//! The tokens Claude used, by UTC day and by payer (ADR-2610081500): a package for `claude:sample`,
//! `agent::BUILDER` for Make an app. Kept in `usage.json` for the last `KEEP_DAYS` days.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const KEEP_DAYS: usize = 31;

/// The tokens of one or more requests. `input` excludes the cached parts, as the APIs count it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub requests: u64,
}

impl Tokens {
    /// One reply's `usage`, as both the Anthropic API and Bedrock send it.
    pub fn of_reply(reply: &Value) -> Tokens {
        let u = &reply["usage"];
        let n = |k: &str| u[k].as_u64().unwrap_or(0);
        Tokens {
            input: n("input_tokens"),
            output: n("output_tokens"),
            cache_write: n("cache_creation_input_tokens"),
            cache_read: n("cache_read_input_tokens"),
            requests: 1,
        }
    }

    /// What a cap counts: every token Claude read, cached or not, and every token it wrote.
    pub fn total(&self) -> u64 {
        self.input.saturating_add(self.output).saturating_add(self.cache_write).saturating_add(self.cache_read)
    }

    fn add(&mut self, t: &Tokens) {
        self.input = self.input.saturating_add(t.input);
        self.output = self.output.saturating_add(t.output);
        self.cache_write = self.cache_write.saturating_add(t.cache_write);
        self.cache_read = self.cache_read.saturating_add(t.cache_read);
        self.requests = self.requests.saturating_add(t.requests);
    }

    fn to_json(self) -> Value {
        json!({ "input": self.input, "output": self.output, "cache_write": self.cache_write,
                "cache_read": self.cache_read, "requests": self.requests, "total": self.total() })
    }
}

/// Days as `YYYY-MM-DD`, which sort in time order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageBook {
    days: BTreeMap<String, BTreeMap<String, Tokens>>,
}

impl UsageBook {
    /// Adds a reply's tokens, and forgets days older than the last `KEEP_DAYS`.
    pub fn add(&mut self, day: &str, payer: &str, t: &Tokens) {
        self.days.entry(day.into()).or_default().entry(payer.into()).or_default().add(t);
        while self.days.len() > KEEP_DAYS {
            self.days.pop_first();
        }
    }

    /// The tokens `payer` used on `day`, as a cap counts them.
    pub fn used(&self, day: &str, payer: &str) -> u64 {
        self.days.get(day).and_then(|d| d.get(payer)).map(Tokens::total).unwrap_or(0)
    }

    /// {today, days: [{day, payers: {payer: tokens}}], newest first, totals: {payer: tokens}}.
    pub fn report(&self, today: &str) -> Value {
        let mut totals: BTreeMap<&str, Tokens> = BTreeMap::new();
        let days: Vec<Value> = self
            .days
            .iter()
            .rev()
            .map(|(day, payers)| {
                for (p, t) in payers {
                    totals.entry(p).or_default().add(t);
                }
                json!({ "day": day, "payers": payers.iter().map(|(p, t)| (p.clone(), t.to_json())).collect::<serde_json::Map<_, _>>() })
            })
            .collect();
        let today_payers = self.days.get(today).map(|d| d.iter().map(|(p, t)| (p.clone(), t.to_json())).collect::<serde_json::Map<_, _>>());
        json!({ "today": today, "today_payers": today_payers.unwrap_or_default(), "days": days,
                "totals": totals.into_iter().map(|(p, t)| (p.to_string(), t.to_json())).collect::<serde_json::Map<_, _>>() })
    }
}

/// The UTC day of a Unix time, as `YYYY-MM-DD` (Howard Hinnant's civil_from_days).
pub fn day_of(unix_secs: u64) -> String {
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_day_of_matches_known_dates() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(951_782_400), "2000-02-29");
        assert_eq!(day_of(1_791_417_599), "2026-10-07");
        assert_eq!(day_of(1_791_417_600), "2026-10-08");
    }

    #[test]
    fn usage_reads_both_apis_usage_and_counts_every_token() {
        let reply = json!({ "usage": { "input_tokens": 10, "output_tokens": 5, "cache_creation_input_tokens": 100, "cache_read_input_tokens": 1000 } });
        let t = Tokens::of_reply(&reply);
        assert_eq!((t.input, t.output, t.cache_write, t.cache_read, t.requests), (10, 5, 100, 1000, 1));
        assert_eq!(t.total(), 1115);
        assert_eq!(Tokens::of_reply(&json!({})).total(), 0, "a reply without usage counts as a request of 0 tokens");
    }

    #[test]
    fn usage_is_kept_per_day_and_payer() {
        let mut b = UsageBook::default();
        let t = Tokens { input: 7, output: 3, requests: 1, ..Default::default() };
        b.add("2026-10-08", "notes", &t);
        b.add("2026-10-08", "notes", &t);
        b.add("2026-10-08", "Make an app", &t);
        b.add("2026-10-07", "notes", &t);
        assert_eq!(b.used("2026-10-08", "notes"), 20);
        assert_eq!(b.used("2026-10-07", "notes"), 10);
        assert_eq!(b.used("2026-10-09", "notes"), 0);
        let r = b.report("2026-10-08");
        assert_eq!(r["days"][0]["day"], "2026-10-08", "newest first");
        assert_eq!(r["totals"]["notes"]["total"], 30);
        assert_eq!(r["totals"]["notes"]["requests"], 3);
        assert_eq!(r["today_payers"]["Make an app"]["total"], 10);
    }

    #[test]
    fn usage_keeps_the_last_31_days() {
        let mut b = UsageBook::default();
        for d in 1..=40u64 {
            b.add(&day_of(d * 86_400), "a", &Tokens { input: 1, requests: 1, ..Default::default() });
        }
        assert_eq!(b.days.len(), KEEP_DAYS);
        assert_eq!(b.used(&day_of(86_400 * 9), "a"), 0, "day 9 is gone");
        assert_eq!(b.used(&day_of(86_400 * 10), "a"), 1, "day 10 is the oldest kept");
    }

    #[test]
    fn usage_counts_saturate_instead_of_wrapping() {
        let mut b = UsageBook::default();
        let huge = Tokens { input: u64::MAX, output: 1, requests: 1, ..Default::default() };
        b.add("d", "a", &huge);
        b.add("d", "a", &huge);
        assert_eq!(b.used("d", "a"), u64::MAX);
    }
}
