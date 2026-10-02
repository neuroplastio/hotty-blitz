//! The in-band resource store and the only resource loader documents get
//! (SPEC §12): `cid:<name>` from the store, `data:` inline, and from the
//! network what the policy allows (SPEC §7.2; policy.rs, fetch.rs). Nothing
//! else: every other request fails as a missing resource does.
//!
//! The loader also decodes the frames of animated images (anim.rs) as it
//! hands them to a document, and keeps them for the document's surface to
//! play ([`Store::take_animations`]).

use crate::anim::{self, Animation};
use crate::fetch;
use crate::policy::{Directive, Policy};
use base64::Engine as _;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use url::Url;

#[derive(Clone)]
pub struct Resource {
    pub mime: String,
    pub bytes: Bytes,
}

/// A document's request, parked until its resource arrives.
type Waiting = Vec<(usize, Box<dyn NetHandler>)>;

#[derive(Default)]
struct Inner {
    resources: HashMap<String, Resource>,
    /// Requests for resources that have not arrived yet.
    waiting: HashMap<String, Waiting>,
    /// Which documents asked for which resource, so a re-send can reach them.
    users: HashMap<String, BTreeSet<usize>>,
    total: usize,
    /// Decoded animations by URL, while a surface plays them; `None`: the
    /// `cid:` resource is not animated. A resource sent again is decoded again.
    animations: HashMap<String, Option<Weak<Animation>>>,
    /// Animations documents loaded, waiting for their surfaces.
    started: Vec<(usize, String, Arc<Animation>)>,
    /// The host's half of the network policy (SPEC §7.2).
    host_policy: Policy,
    /// Each live document's half, from its `<meta name="hotty-network">`.
    policies: HashMap<usize, Policy>,
    /// Network requests a document made while it was being built, before
    /// its half of the policy was known.
    parked: Vec<(usize, Request, Box<dyn NetHandler>)>,
    /// Documents that got something from the network since the host last
    /// looked: they have messages to handle, and draw again.
    delivered: BTreeSet<usize>,
    limits: fetch::Limits,
}

/// Wakes the thread that renders, from a fetching thread.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Per-session resource store, shared by every surface of a host.
pub struct Store {
    inner: Mutex<Inner>,
    pub quota: usize,
    /// `started` has entries: checked without the lock on every render.
    has_started: AtomicBool,
    /// `delivered` has entries, likewise.
    has_delivered: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl Store {
    pub fn new(quota: usize) -> Arc<Store> {
        Arc::new(Store {
            inner: Mutex::new(Inner::default()),
            quota,
            has_started: AtomicBool::new(false),
            has_delivered: AtomicBool::new(false),
            waker: Mutex::new(None),
        })
    }

    /// Stores (or replaces) a resource. Returns the documents that use it, so
    /// the caller can have them process the delivered bytes and restyle.
    pub fn put(&self, name: &str, mime: &str, bytes: Vec<u8>) -> Result<BTreeSet<usize>, String> {
        let bytes = Bytes::from(bytes);
        let (waiting, users) = {
            let mut inner = self.inner.lock().unwrap();
            let old = inner.resources.get(name).map_or(0, |r| r.bytes.len());
            let total = inner.total - old + bytes.len();
            if total > self.quota {
                return Err(format!(
                    "resource store quota of {} bytes exceeded",
                    self.quota
                ));
            }
            inner.total = total;
            inner.resources.insert(
                name.to_string(),
                Resource {
                    mime: mime.to_string(),
                    bytes: bytes.clone(),
                },
            );
            let waiting = inner.waiting.remove(name).unwrap_or_default();
            let users = inner.users.get(name).cloned().unwrap_or_default();
            inner.animations.remove(&format!("cid:{name}"));
            (waiting, users)
        };
        // Deliver outside the lock: a stylesheet handler may fetch @imports.
        let url = format!("cid:{name}");
        for (doc_id, handler) in waiting {
            self.load(doc_id, &url, &bytes);
            handler.bytes(url.clone(), bytes.clone());
        }
        Ok(users)
    }

    pub fn remove(&self, name: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        inner.animations.remove(&format!("cid:{name}"));
        match inner.resources.remove(name) {
            Some(r) => {
                inner.total -= r.bytes.len();
                true
            }
            None => false,
        }
    }

    pub fn forget_document(&self, doc_id: usize) {
        let mut inner = self.inner.lock().unwrap();
        for users in inner.users.values_mut() {
            users.remove(&doc_id);
        }
        for waiting in inner.waiting.values_mut() {
            waiting.retain(|(d, _)| *d != doc_id);
        }
        inner.started.retain(|(d, _, _)| *d != doc_id);
        inner.policies.remove(&doc_id);
        inner.parked.retain(|(d, _, _)| *d != doc_id);
        inner.delivered.remove(&doc_id);
    }

    /// Document `doc_id` loads `bytes` from `url`: if they are an animated
    /// image, its surface gets the frames to play (`take_animations`).
    /// Each resource is decoded once, while anything plays it.
    fn load(&self, doc_id: usize, url: &str, bytes: &[u8]) {
        let (known, used) = {
            let mut inner = self.inner.lock().unwrap();
            inner
                .animations
                .retain(|_, a| a.as_ref().is_none_or(|w| w.strong_count() > 0));
            let used: usize = inner
                .animations
                .values()
                .filter_map(|a| a.as_ref()?.upgrade())
                .map(|a| a.bytes)
                .sum();
            (inner.animations.get(url).cloned(), used)
        };
        let anim = match known {
            Some(None) => return,
            Some(Some(weak)) => weak.upgrade(),
            None => None,
        };
        let anim = match anim {
            Some(a) => a,
            None => {
                let left = anim::BUDGET.saturating_sub(used);
                let decoded = anim::decode(bytes, left);
                let mut inner = self.inner.lock().unwrap();
                match decoded {
                    anim::Decoded::Animated(a) => {
                        let a = Arc::new(a);
                        inner
                            .animations
                            .insert(url.to_string(), Some(Arc::downgrade(&a)));
                        a
                    }
                    // Still, or not an image, or too big for any budget: no
                    // need to look again until it is sent again. Too big for
                    // what is left of the budget: looked at again next time.
                    // (data: URLs never change, and are not remembered.)
                    anim::Decoded::Still | anim::Decoded::TooBig if url.starts_with("cid:") => {
                        if left >= anim::MAX_BYTES || matches!(decoded, anim::Decoded::Still) {
                            inner.animations.insert(url.to_string(), None);
                        }
                        return;
                    }
                    _ => return,
                }
            }
        };
        let mut inner = self.inner.lock().unwrap();
        inner.started.push((doc_id, url.to_string(), anim));
        self.has_started.store(true, Ordering::Release);
    }

    /// The animated images document `doc_id` loaded since the last call,
    /// by URL, to play.
    pub fn take_animations(&self, doc_id: usize) -> Vec<(String, Arc<Animation>)> {
        if !self.has_started.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut inner = self.inner.lock().unwrap();
        let mut mine = Vec::new();
        inner.started.retain(|(d, url, a)| {
            if *d == doc_id {
                mine.push((url.clone(), a.clone()));
            }
            *d != doc_id
        });
        if inner.started.is_empty() {
            self.has_started.store(false, Ordering::Release);
        }
        mine
    }

    pub fn total_bytes(&self) -> usize {
        self.inner.lock().unwrap().total
    }

    /// The host's half of the network policy. Documents already shown keep
    /// theirs and are held to the new one from their next request.
    pub fn set_host_policy(&self, policy: Policy) {
        self.inner.lock().unwrap().host_policy = policy;
    }

    pub fn host_policy(&self) -> Policy {
        self.inner.lock().unwrap().host_policy.clone()
    }

    pub fn set_limits(&self, limits: fetch::Limits) {
        self.inner.lock().unwrap().limits = limits;
    }

    /// Called from a fetching thread when something a document asked for
    /// arrives: the renderer then asks `has_delivered`. Once this returns,
    /// the old waker is not called again.
    pub fn set_waker(&self, waker: Option<Waker>) {
        *self.waker.lock().unwrap() = waker;
    }

    /// Document `doc_id` is built, and asks for the network what `policy`
    /// says (its `<meta name="hotty-network">`): the requests it made while
    /// it was being built go now.
    pub fn set_document_policy(self: &Arc<Self>, doc_id: usize, policy: Policy) {
        let parked = {
            let mut inner = self.inner.lock().unwrap();
            inner.policies.insert(doc_id, policy);
            let (mine, rest) = std::mem::take(&mut inner.parked)
                .into_iter()
                .partition(|(d, _, _)| *d == doc_id);
            inner.parked = rest;
            mine
        };
        for (_, request, handler) in parked {
            self.request(doc_id, request, handler);
        }
    }

    /// Whether both halves of the policy allow `url` for `directive`, for
    /// document `doc_id` (SPEC §7.2).
    fn allows(&self, doc_id: usize, directive: Directive, url: &Url) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.host_policy.allows(directive, url)
            && inner
                .policies
                .get(&doc_id)
                .is_some_and(|p| p.allows(directive, url))
    }

    /// A request for an `http` or `https` URL: fetched if the policy allows
    /// it, failed as a missing resource otherwise.
    fn request(self: &Arc<Self>, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        {
            let mut inner = self.inner.lock().unwrap();
            if !inner.policies.contains_key(&doc_id) {
                inner.parked.push((doc_id, request, handler));
                return;
            }
        }
        let url = request.url;
        let directive = Directive::of(request.destination)
            .filter(|&d| fetch::fetchable(&url) && self.allows(doc_id, d, &url));
        let Some(directive) = directive else {
            handler.bytes(url.to_string(), Bytes::new());
            return;
        };
        let limits = self.inner.lock().unwrap().limits;
        let store = Arc::downgrade(self);
        let allows: fetch::Allows = Arc::new(move |u: &Url| {
            store
                .upgrade()
                .is_some_and(|s| s.allows(doc_id, directive, u))
        });
        let store = Arc::downgrade(self);
        let asked = url.to_string();
        fetch::get(fetch::Want {
            url,
            accept: match directive {
                Directive::Img => "image/*,*/*;q=0.8",
                Directive::Style => "text/css,*/*;q=0.1",
                Directive::Font | Directive::Media => "*/*",
            },
            limits,
            allows,
            done: Box::new(move |bytes| {
                if let Some(store) = store.upgrade() {
                    store.arrived(doc_id, &asked, bytes.unwrap_or_default(), handler);
                }
            }),
        });
    }

    /// What document `doc_id` fetched arrived (empty: it failed), on a
    /// fetching thread: the document gets it as it would a resource, and
    /// the renderer is woken to draw it.
    fn arrived(&self, doc_id: usize, url: &str, bytes: Bytes, handler: Box<dyn NetHandler>) {
        if !self.inner.lock().unwrap().policies.contains_key(&doc_id) {
            return;
        }
        if !bytes.is_empty() {
            self.load(doc_id, url, &bytes);
        }
        handler.bytes(url.to_string(), bytes);
        self.inner.lock().unwrap().delivered.insert(doc_id);
        self.has_delivered.store(true, Ordering::Release);
        // Under the lock: once `set_waker` has replaced it, the old one is
        // never called again (its context may be gone).
        if let Some(wake) = self.waker.lock().unwrap().as_ref() {
            wake();
        }
    }

    /// Whether a document got something from the network that it has not
    /// drawn yet.
    pub fn has_delivered(&self) -> bool {
        self.has_delivered.load(Ordering::Acquire)
    }

    /// The documents that got something from the network since the last
    /// call.
    pub fn take_delivered(&self) -> BTreeSet<usize> {
        if !self.has_delivered() {
            return BTreeSet::new();
        }
        let mut inner = self.inner.lock().unwrap();
        self.has_delivered.store(false, Ordering::Release);
        std::mem::take(&mut inner.delivered)
    }
}

/// The `NetProvider` every document is built with.
pub struct Provider(pub Arc<Store>);

impl NetProvider for Provider {
    fn fetch(&self, doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        if matches!(request.url.scheme(), "http" | "https") {
            return self.0.request(doc_id, request, handler);
        }
        let url = request.url;
        match url.scheme() {
            "cid" => {
                let name = url.path().to_string();
                let found = {
                    let mut inner = self.0.inner.lock().unwrap();
                    inner.users.entry(name.clone()).or_default().insert(doc_id);
                    match inner.resources.get(&name) {
                        Some(r) => Some(r.bytes.clone()),
                        None => {
                            inner
                                .waiting
                                .entry(name)
                                .or_default()
                                .push((doc_id, handler));
                            return;
                        }
                    }
                };
                if let Some(bytes) = found {
                    self.0.load(doc_id, url.as_str(), &bytes);
                    handler.bytes(url.to_string(), bytes);
                }
            }
            "data" => {
                let bytes = decode_data_url(url.as_str()).unwrap_or_default();
                self.0.load(doc_id, url.as_str(), &bytes);
                handler.bytes(url.to_string(), Bytes::from(bytes));
            }
            // SPEC §12: fail closed. Empty bytes rather than silence, so that a
            // stylesheet which can never load does not block painting forever.
            _ => handler.bytes(url.to_string(), Bytes::new()),
        }
    }
}

fn decode_data_url(url: &str) -> Option<Vec<u8>> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if meta.ends_with(";base64") {
        base64::engine::general_purpose::STANDARD
            .decode(data.trim())
            .ok()
    } else {
        Some(percent_decode(data))
    }
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}
