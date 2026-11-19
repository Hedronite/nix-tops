//! The warning ledger (`[[warnings.allow]]`, protocol section 3.1): an acknowledgement with an
//! expiry. An expired entry fails the gate whether or not its warning appears.

use crate::report::AckEntry;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub matches: String,
    pub reason: String,
    pub until: String,
}

fn field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The ledger of a parsed `nix-tops.toml`.
pub fn entries(doc: &Value) -> Vec<Entry> {
    let allow = doc.pointer("/warnings/allow").and_then(Value::as_array);
    let list = allow.into_iter().flatten();
    list.map(|e| Entry {
        matches: field(e, "match"),
        reason: field(e, "reason"),
        until: field(e, "until"),
    })
    .collect()
}

impl Entry {
    /// An entry whose `until` is before the run date (both `YYYY-MM-DD`, UTC) no longer applies.
    pub fn expired(&self, today: &str) -> bool {
        self.until.as_str() < today
    }

    pub fn ack(&self, today: &str) -> AckEntry {
        AckEntry {
            kind: "ledger",
            matches: self.matches.clone(),
            reason: self.reason.clone(),
            until: self.until.clone(),
            expired: self.expired(today),
        }
    }
}

/// The first unexpired entry whose `match` is a substring of `message`.
pub fn matching<'a>(entries: &'a [Entry], message: &str, today: &str) -> Option<&'a Entry> {
    entries
        .iter()
        .find(|e| !e.expired(today) && message.contains(&e.matches))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(m: &str, until: &str) -> Entry {
        Entry {
            matches: m.into(),
            reason: "r".into(),
            until: until.into(),
        }
    }

    #[test]
    fn entries_read_the_allow_list() {
        let doc =
            json!({"warnings": {"allow": [{"match": "a", "reason": "b", "until": "2030-01-01"}]}});
        assert_eq!(
            entries(&doc),
            [Entry {
                matches: "a".into(),
                reason: "b".into(),
                until: "2030-01-01".into()
            }]
        );
        assert!(entries(&json!({})).is_empty());
    }

    #[test]
    fn an_entry_expires_the_day_after_until() {
        let e = entry("x", "2026-11-18");
        assert!(!e.expired("2026-11-18"));
        assert!(e.expired("2026-11-19"));
        assert!(!e.expired("2026-01-01"));
    }

    #[test]
    fn matching_is_a_substring_and_skips_expired_entries() {
        let list = vec![entry("gone", "2000-01-01"), entry("renamed", "2099-01-01")];
        assert_eq!(
            matching(&list, "option has been renamed to y", "2026-11-18")
                .map(|e| e.matches.as_str()),
            Some("renamed")
        );
        assert!(matching(&list, "gone away", "2026-11-18").is_none());
        assert!(matching(&list, "nothing", "2026-11-18").is_none());
    }

    #[test]
    fn the_ack_carries_the_entry_and_whether_it_expired() {
        let ack = entry("m", "2000-01-01").ack("2026-11-18");
        assert_eq!(
            (ack.kind, ack.matches.as_str(), ack.expired),
            ("ledger", "m", true)
        );
        assert!(!entry("m", "2099-01-01").ack("2026-11-18").expired);
    }
}
