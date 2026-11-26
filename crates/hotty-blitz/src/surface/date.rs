//! Date and time inputs (SPEC §10.2): Blitz lays one out as a one-line field
//! showing its value, ISO 8601 as HTML gives it (`blitz_dom::date_time`), and
//! this host edits it a number at a time, as a browser's date field does.
//! The field's text is the truth: its value is the text once that is a valid
//! one of its type's, and empty until then (HTML's value sanitization), and a
//! number not yet given shows as its letters (`yyyy`, `mm`, `--`).
//!
//! The keys are §10.2's for a date or time type: a digit types into the number
//! the selection is on, and moves on once no other digit fits; a separator
//! moves on; Left and Right move between the numbers; Up and Down step the
//! number, wrapping; Page Up and Page Down step it further; Home and End give
//! it its least and greatest; Backspace and Delete clear it; Enter submits the
//! form as in a text field. Each change of the value sends `change`, and
//! `input` with `data-on~=input`, as a select's does.

use super::Surface;
use crate::body::Detail;
use crate::input::{Event, Key, KeyName, Mods};
use blitz_dom::date_time;
use blitz_dom::{NodeId, local_name};

/// One number of a field: its range, its digits, what shows while it has
/// none, and how far Page Up and Page Down step it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Unit {
    Year,
    Month,
    Day,
    Week,
    Hour,
    Minute,
    Second,
    Milli,
}

impl Unit {
    fn width(self) -> usize {
        match self {
            Unit::Year => 4,
            Unit::Milli => 3,
            _ => 2,
        }
    }

    fn blank(self) -> &'static str {
        match self {
            Unit::Year => "yyyy",
            Unit::Month => "mm",
            Unit::Day => "dd",
            Unit::Week => "ww",
            Unit::Milli => "---",
            Unit::Hour | Unit::Minute | Unit::Second => "--",
        }
    }

    fn min(self) -> u32 {
        match self {
            Unit::Year | Unit::Month | Unit::Day | Unit::Week => 1,
            _ => 0,
        }
    }

    /// The greatest, given the year and month the field has (a day of the
    /// month, a week of the year).
    fn max(self, year: Option<u32>, month: Option<u32>) -> u32 {
        match self {
            // HTML's greatest year, in the date of 2^53 ms after the epoch.
            Unit::Year => 275_760,
            Unit::Month => 12,
            Unit::Day => match month {
                Some(m) => date_time::days_in_month(year.unwrap_or(2000), m),
                None => 31,
            },
            Unit::Week => year.map_or(53, date_time::weeks_in_year),
            Unit::Hour => 23,
            Unit::Minute | Unit::Second => 59,
            Unit::Milli => 999,
        }
    }

    fn page(self) -> u32 {
        match self {
            Unit::Year => 10,
            Unit::Month => 3,
            Unit::Day => 7,
            Unit::Week => 4,
            Unit::Hour => 6,
            Unit::Minute | Unit::Second => 15,
            Unit::Milli => 100,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Sep(&'static str),
    Num(Unit),
}

/// The tokens of a field of type `ty` whose text is `text`: a time has
/// seconds, and a fraction, when its text does.
fn layout(ty: &str, text: &str) -> Vec<Token> {
    use Token::{Num, Sep};
    let time_part = match ty {
        "time" => text,
        "datetime-local" => text.split_once(['T', ' ']).map_or("", |(_, t)| t),
        _ => "",
    };
    let mut time = vec![Num(Unit::Hour), Sep(":"), Num(Unit::Minute)];
    if time_part.matches(':').count() >= 2 {
        time.extend([Sep(":"), Num(Unit::Second)]);
        if time_part.contains('.') {
            time.extend([Sep("."), Num(Unit::Milli)]);
        }
    }
    let date = [
        Num(Unit::Year),
        Sep("-"),
        Num(Unit::Month),
        Sep("-"),
        Num(Unit::Day),
    ];
    match ty {
        "date" => date.to_vec(),
        "month" => vec![Num(Unit::Year), Sep("-"), Num(Unit::Month)],
        "week" => vec![Num(Unit::Year), Sep("-W"), Num(Unit::Week)],
        "time" => time,
        "datetime-local" => {
            let mut t = date.to_vec();
            t.push(Sep("T"));
            t.extend(time);
            t
        }
        _ => Vec::new(),
    }
}

/// The numbers of a field: each unit and its number, if it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    tokens: Vec<Token>,
    nums: Vec<Option<u32>>,
}

impl Field {
    /// The field of type `ty` showing `text`; a number that does not read
    /// as one has none.
    fn parse(ty: &str, text: &str) -> Field {
        let tokens = layout(ty, text);
        let mut nums = Vec::new();
        let mut rest = text;
        for (i, t) in tokens.iter().enumerate() {
            match *t {
                Token::Sep("T") => rest = rest.strip_prefix(['T', ' ']).unwrap_or(""),
                Token::Sep(s) => rest = rest.strip_prefix(s).unwrap_or(""),
                Token::Num(_) => {
                    // A number runs to its separator, which a datetime's
                    // `T` may be a space for.
                    let end = match tokens.get(i + 1) {
                        Some(Token::Sep("T")) => rest.find(['T', ' ']),
                        Some(Token::Sep(s)) => rest.find(s),
                        _ => None,
                    }
                    .unwrap_or(rest.len());
                    let s = &rest[..end];
                    let n = (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                        .then(|| s.parse().ok())
                        .flatten();
                    nums.push(n);
                    rest = &rest[end..];
                }
            }
        }
        Field { tokens, nums }
    }

    fn units(&self) -> Vec<Unit> {
        self.tokens
            .iter()
            .filter_map(|t| match t {
                Token::Num(u) => Some(*u),
                Token::Sep(_) => None,
            })
            .collect()
    }

    fn get(&self, unit: Unit) -> Option<u32> {
        let i = self.units().iter().position(|&u| u == unit)?;
        self.nums[i]
    }

    fn max(&self, i: usize) -> u32 {
        self.units()[i].max(self.get(Unit::Year), self.get(Unit::Month))
    }

    /// The field's text, and the byte range of each number in it.
    fn text(&self) -> (String, Vec<(usize, usize)>) {
        let mut s = String::new();
        let mut ranges = Vec::new();
        let mut n = 0;
        for t in &self.tokens {
            match *t {
                Token::Sep(sep) => s.push_str(sep),
                Token::Num(u) => {
                    let start = s.len();
                    match self.nums[n] {
                        Some(v) => s.push_str(&format!("{v:0w$}", w = u.width())),
                        None => s.push_str(u.blank()),
                    }
                    ranges.push((start, s.len()));
                    n += 1;
                }
            }
        }
        (s, ranges)
    }

    /// Steps number `i` by `by`, wrapping in its range; one it does not
    /// have yet starts at its least going up, its greatest going down (a
    /// year at `year`, this year).
    fn step(&mut self, i: usize, by: i64, year: u32) {
        let unit = self.units()[i];
        let (min, max) = (unit.min() as i64, self.max(i) as i64);
        let next = match self.nums[i] {
            None if unit == Unit::Year => year as i64,
            None if by > 0 => min,
            None => max,
            Some(v) => {
                let span = max - min + 1;
                let v = (v as i64).clamp(min, max);
                if unit == Unit::Year {
                    (v + by).clamp(min, max)
                } else {
                    min + (v - min + by).rem_euclid(span)
                }
            }
        };
        self.nums[i] = Some(next as u32);
    }
}

/// This year, in UTC.
fn this_year() -> u32 {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2)) as u32
}

/// The field's number the selection `(anchor, focus)` is on: the one it
/// starts in, and the first for a caret at either end of the text, where
/// focus puts it.
fn number_at(ranges: &[(usize, usize)], len: usize, anchor: usize, focus: usize) -> usize {
    let at = anchor.min(focus);
    if anchor == focus && (at == 0 || at >= len) {
        return 0;
    }
    ranges
        .iter()
        .position(|&(s, e)| at >= s && at <= e)
        .or_else(|| ranges.iter().position(|&(s, _)| s > at))
        .unwrap_or(0)
}

impl Surface {
    /// A key for the focused date or time field `id` (module doc). `None`
    /// when the key is not the field's.
    pub(super) fn date_key(&mut self, id: NodeId, key: &Key) -> Option<Vec<Event>> {
        if key.mods.ctrl || key.mods.alt || key.mods.super_ {
            return None;
        }
        let ty = self.date_type(id)?;
        let (text, anchor, focus) = self.doc.text_input_selection(id)?;
        let mut field = Field::parse(ty, &text);
        let (_, ranges) = field.text();
        let count = ranges.len();
        let mut at = match self.date_typed {
            Some((n, i, _)) if n == id && i < count => i,
            _ => number_at(&ranges, text.len(), anchor, focus),
        };
        let mut typed = match self.date_typed {
            Some((n, i, t)) if n == id && i == at => t,
            _ => 0,
        };
        let before = date_time::sanitize(ty, &text);
        match &key.name {
            KeyName::Left => {
                at = at.saturating_sub(1);
                typed = 0;
            }
            KeyName::Right => {
                at = (at + 1).min(count - 1);
                typed = 0;
            }
            KeyName::Up | KeyName::Down | KeyName::PageUp | KeyName::PageDown => {
                let unit = field.units()[at];
                let by = match key.name {
                    KeyName::Up => 1,
                    KeyName::Down => -1,
                    KeyName::PageUp => unit.page() as i64,
                    _ => -(unit.page() as i64),
                };
                field.step(at, by, this_year());
                typed = 0;
            }
            KeyName::Home => {
                field.nums[at] = Some(field.units()[at].min());
                typed = 0;
            }
            KeyName::End => {
                field.nums[at] = Some(field.max(at));
                typed = 0;
            }
            KeyName::Backspace | KeyName::Delete => {
                field.nums[at] = None;
                typed = 0;
            }
            KeyName::Enter => {
                // As in a text field: Blitz submits the form (Surface::field_action).
                self.date_typed = None;
                return Some(self.drive_key(&Key::new(KeyName::Enter, Mods::default())));
            }
            KeyName::Space => {}
            KeyName::Char(c) => {
                for ch in c.chars() {
                    if let Some(d) = ch.to_digit(10) {
                        let unit = field.units()[at];
                        let max = field.max(at);
                        let old = field.nums[at].filter(|_| typed > 0).unwrap_or(0);
                        let mut v = old * 10 + d;
                        typed += 1;
                        if v > max.max(unit.min()) && typed > 1 {
                            v = d;
                            typed = 1;
                        }
                        field.nums[at] = Some(v);
                        // On to the next once no other digit fits.
                        if (typed >= unit.width() || v * 10 > max) && at + 1 < count {
                            at += 1;
                            typed = 0;
                        }
                    } else if matches!(ch, '-' | ':' | '/' | '.' | ',' | ' ' | 'T' | 't' | 'W')
                        && typed > 0
                        && at + 1 < count
                    {
                        // A separator ends a number begun; after one that
                        // moved on by itself, it is the one typed past.
                        at += 1;
                        typed = 0;
                    }
                }
            }
            _ => return None,
        }
        self.date_typed = Some((id, at, typed));
        let (new_text, ranges) = field.text();
        let (s, e) = ranges[at];
        self.doc.set_text_input(id, &new_text, s, e);
        self.damage.touch(&self.doc, id, self.viewport.2 as f64);
        self.set_dirty();
        let value = date_time::sanitize(ty, &new_text);
        if value == before {
            return Some(Vec::new());
        }
        let rev = super::field::bump(&mut self.revs, &self.doc, id);
        let mut events = Vec::new();
        if let Some(target) = self.id_of(id) {
            if self.listens(id, "input") {
                events.push(Event {
                    kind: "input",
                    target: target.clone(),
                    detail: Some(Detail::Value {
                        value: value.clone(),
                        rev,
                        sel: None,
                    }),
                });
            }
            events.push(Event {
                kind: "change",
                target,
                detail: Some(Detail::Value {
                    value,
                    rev,
                    sel: None,
                }),
            });
        }
        Some(events)
    }

    /// The type of `id`, if it is a date or time input.
    pub(super) fn date_type(&self, id: NodeId) -> Option<&'static str> {
        let el = self.doc.get_node(id)?.element_data()?;
        if el.name.local != local_name!("input") {
            return None;
        }
        date_time::date_time_type(el.attr(local_name!("type")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(ty: &str, s: &str) -> String {
        Field::parse(ty, s).text().0
    }

    #[test]
    fn parses_and_shows() {
        assert_eq!(text("date", "2026-11-17"), "2026-11-17");
        assert_eq!(text("date", ""), "yyyy-mm-dd");
        assert_eq!(text("date", "2026-mm-dd"), "2026-mm-dd");
        assert_eq!(text("time", "09:30"), "09:30");
        assert_eq!(text("time", ""), "--:--");
        assert_eq!(text("time", "09:30:05.250"), "09:30:05.250");
        assert_eq!(
            text("datetime-local", "2026-11-17T09:30"),
            "2026-11-17T09:30"
        );
        assert_eq!(
            text("datetime-local", "2026-11-17 09:30"),
            "2026-11-17T09:30"
        );
        assert_eq!(text("datetime-local", ""), "yyyy-mm-ddT--:--");
        assert_eq!(text("month", "2026-11"), "2026-11");
        assert_eq!(text("week", "2026-W47"), "2026-W47");
        assert_eq!(text("week", ""), "yyyy-Www");
    }

    #[test]
    fn steps_wrap() {
        let mut f = Field::parse("date", "2026-12-31");
        f.step(1, 1, 2000);
        assert_eq!(f.text().0, "2026-01-31");
        f.step(2, 1, 2000);
        assert_eq!(f.text().0, "2026-01-01");
        f.step(2, -1, 2000);
        assert_eq!(f.text().0, "2026-01-31");
        let mut f = Field::parse("time", "");
        f.step(0, -1, 2000);
        assert_eq!(f.text().0, "23:--");
        let mut f = Field::parse("date", "");
        f.step(0, 1, 2031);
        assert_eq!(f.text().0, "2031-mm-dd");
    }

    #[test]
    fn caret_at_an_end_is_on_the_first() {
        let (_, r) = Field::parse("date", "2026-11-17").text();
        assert_eq!(number_at(&r, 10, 10, 10), 0);
        assert_eq!(number_at(&r, 10, 0, 0), 0);
        assert_eq!(number_at(&r, 10, 5, 7), 1);
        assert_eq!(number_at(&r, 10, 8, 10), 2);
    }
}
