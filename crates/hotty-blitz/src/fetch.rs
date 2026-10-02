//! Fetching from the network (SPEC §7.2), for every host in the process: a
//! few threads, off every terminal's render thread, and one cache, so that a
//! URL several documents show is fetched once while the cache keeps it.
//!
//! Only `http` and `https`, with no referrer, cookies or credentials. A
//! response over [`MAX_BYTES`], or one that takes longer than [`TIMEOUT`]
//! with its redirects, fails. Whether a URL may be fetched at all is the
//! policy's to say (net.rs); a redirect is followed only to a URL it allows
//! too, and reaches only those waiting whose policy allows every URL on the
//! way.

use blitz_traits::net::Bytes;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use url::Url;

/// The most a response may hold: past it, the fetch fails.
pub const MAX_BYTES: usize = 8 << 20;
/// The longest a fetch may take, its redirects and body included.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// Redirects followed before a fetch fails.
const MAX_REDIRECTS: usize = 5;
/// What the cache keeps of finished fetches; the least recently used go
/// first.
const CACHE_BYTES: usize = 64 << 20;
/// Fetches at once.
const WORKERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: usize,
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_bytes: MAX_BYTES,
            timeout: TIMEOUT,
        }
    }
}

/// Whether a URL is one this host ever fetches: `http` or `https`, with no
/// credentials in it, and not under the base every document has by default
/// (`https://hotty.invalid/`, SPEC §7.3), which names nothing.
pub fn fetchable(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|h| h != "hotty.invalid")
}

/// Whether the policy of whoever waits allows a URL: the one asked for was
/// checked before; this one is for the URLs a fetch is redirected to.
pub type Allows = Arc<dyn Fn(&Url) -> bool + Send + Sync>;

/// A request for a URL, and what to do with the bytes: `None` when the
/// fetch failed, or went somewhere `allows` does not.
pub struct Want {
    pub url: Url,
    /// The `Accept` header.
    pub accept: &'static str,
    pub limits: Limits,
    pub allows: Allows,
    pub done: Box<dyn FnOnce(Option<Bytes>) + Send>,
}

struct Waiter {
    allows: Allows,
    done: Box<dyn FnOnce(Option<Bytes>) + Send>,
}

impl Waiter {
    fn finish(self, fetched: Option<&Fetched>) {
        let bytes = fetched
            .filter(|f| f.redirects.iter().all(|u| (self.allows)(u)))
            .map(|f| f.bytes.clone());
        (self.done)(bytes);
    }
}

#[derive(Clone)]
struct Fetched {
    /// The URLs it was redirected to, in order.
    redirects: Vec<Url>,
    bytes: Bytes,
}

enum Entry {
    Pending(Vec<Waiter>),
    Done(Fetched),
}

struct Job {
    url: Url,
    accept: &'static str,
    limits: Limits,
    /// The first waiter's policy, for the redirects.
    allows: Allows,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    /// Finished entries, least recently used first.
    order: VecDeque<String>,
    bytes: usize,
    jobs: VecDeque<Job>,
    workers: usize,
    idle: usize,
}

struct Fetcher {
    state: Mutex<State>,
    ready: Condvar,
    agent: ureq::Agent,
}

fn fetcher() -> &'static Fetcher {
    static FETCHER: OnceLock<Fetcher> = OnceLock::new();
    FETCHER.get_or_init(|| Fetcher {
        state: Mutex::new(State::default()),
        ready: Condvar::new(),
        agent: ureq::Agent::config_builder()
            // Followed here, each checked against the policy.
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(concat!("hotty-blitz/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent(),
    })
}

/// Fetches `want.url`, or hands over what the cache has of it. `done` is
/// called on a fetching thread, or on this one from the cache.
pub fn get(want: Want) {
    let f = fetcher();
    let key = want.url.as_str().to_string();
    let waiter = Waiter {
        allows: want.allows.clone(),
        done: want.done,
    };
    let mut st = f.state.lock().unwrap();
    match st.entries.get_mut(&key) {
        Some(Entry::Done(fetched)) => {
            let fetched = fetched.clone();
            if let Some(i) = st.order.iter().position(|k| *k == key) {
                let k = st.order.remove(i).unwrap();
                st.order.push_back(k);
            }
            drop(st);
            waiter.finish(Some(&fetched));
        }
        Some(Entry::Pending(waiters)) => waiters.push(waiter),
        None => {
            st.entries.insert(key, Entry::Pending(vec![waiter]));
            st.jobs.push_back(Job {
                url: want.url,
                accept: want.accept,
                limits: want.limits,
                allows: want.allows,
            });
            if st.idle == 0 && st.workers < WORKERS {
                st.workers += 1;
                let spawned = std::thread::Builder::new()
                    .name("hotty-fetch".into())
                    .spawn(work);
                if spawned.is_err() {
                    st.workers -= 1;
                }
            }
            f.ready.notify_one();
        }
    }
}

fn work() {
    let f = fetcher();
    loop {
        let job = {
            let mut st = f.state.lock().unwrap();
            loop {
                if let Some(job) = st.jobs.pop_front() {
                    break job;
                }
                st.idle += 1;
                st = f.ready.wait(st).unwrap();
                st.idle -= 1;
            }
        };
        let key = job.url.as_str().to_string();
        let fetched = fetch(&f.agent, &job);
        let waiters = {
            let mut st = f.state.lock().unwrap();
            let waiters = match st.entries.remove(&key) {
                Some(Entry::Pending(w)) => w,
                _ => Vec::new(),
            };
            if let Some(fetched) = &fetched {
                st.bytes += fetched.bytes.len();
                st.entries.insert(key.clone(), Entry::Done(fetched.clone()));
                st.order.push_back(key);
                while st.bytes > CACHE_BYTES
                    && let Some(old) = st.order.pop_front()
                {
                    if let Some(Entry::Done(gone)) = st.entries.remove(&old) {
                        st.bytes -= gone.bytes.len();
                    }
                }
            }
            waiters
        };
        for w in waiters {
            w.finish(fetched.as_ref());
        }
    }
}

/// One fetch, following redirects the policy allows: the bytes of a 2xx
/// response, or `None`.
fn fetch(agent: &ureq::Agent, job: &Job) -> Option<Fetched> {
    let deadline = Instant::now() + job.limits.timeout;
    let mut url = job.url.clone();
    let mut redirects = Vec::new();
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let mut resp = agent
            .get(url.as_str())
            .header("Accept", job.accept)
            .config()
            .timeout_global(Some(left))
            .build()
            .call()
            .ok()?;
        let status = resp.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let to = resp.headers().get("location")?.to_str().ok()?;
            let next = url.join(to).ok()?;
            if redirects.len() == MAX_REDIRECTS || !fetchable(&next) || !(job.allows)(&next) {
                return None;
            }
            redirects.push(next.clone());
            url = next;
            continue;
        }
        if !(200..300).contains(&status) {
            return None;
        }
        let max = job.limits.max_bytes;
        if resp.body().content_length().is_some_and(|n| n > max as u64) {
            return None;
        }
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(max as u64)
            .read_to_vec()
            .ok()?;
        return Some(Fetched {
            redirects,
            bytes: Bytes::from(bytes),
        });
    }
}
