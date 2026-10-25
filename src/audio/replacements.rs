//! Literal substitutions applied to the transcript text.
//!
//! `initial_prompt` only conditions Whisper; it cannot force a spelling. A
//! name that still comes out wrong ("Not.cafeta" for "knot.kz") is fixed here,
//! after decoding, deterministically.

use anyhow::Result;
use std::collections::BTreeMap;

/// Upper bound on the number of rules in one call. Matching is a linear scan
/// per position, so this keeps a hostile map from making rendering slow.
pub const MAX_REPLACEMENTS: usize = 200;

/// The `replacements` tool argument: `{ "<wrong>": "<right>", ... }`.
///
/// A newtype rather than a bare map because the tool-schema derive only
/// understands plain types; it calls [`Self::json_schema`] for this one.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct ReplacementMap(pub BTreeMap<String, String>);

impl ReplacementMap {
    /// JSON schema of the argument: an object whose values are strings.
    pub fn json_schema() -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        map.insert("type".to_string(), "object".into());
        map.insert(
            "additionalProperties".to_string(),
            serde_json::json!({ "type": "string" }),
        );
        map
    }
}

/// Validated rules, ready to apply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replacements {
    /// `(from, to)`, longest `from` first so the longest match wins.
    rules: Vec<(String, String)>,
}

impl Replacements {
    /// Validates `map`: NUL bytes are stripped, empty keys and maps larger
    /// than [`MAX_REPLACEMENTS`] are rejected.
    pub fn new(map: &ReplacementMap) -> Result<Self> {
        if map.0.len() > MAX_REPLACEMENTS {
            anyhow::bail!(
                "Too many replacements ({}): at most {MAX_REPLACEMENTS} are allowed",
                map.0.len()
            );
        }
        let mut rules = Vec::with_capacity(map.0.len());
        for (from, to) in &map.0 {
            let from = strip_nul(from);
            if from.is_empty() {
                anyhow::bail!("Invalid replacements: keys must not be empty");
            }
            rules.push((from, strip_nul(to)));
        }
        rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
        Ok(Self { rules })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Replaces every match in `text`, scanning left to right. At each
    /// position the longest key wins, and replaced text is never matched
    /// again, so the result does not depend on the order of the map.
    pub fn apply(&self, text: &str) -> String {
        if self.is_empty() {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(ch) = rest.chars().next() {
            match self
                .rules
                .iter()
                .find(|(from, _)| rest.starts_with(from.as_str()))
            {
                Some((from, to)) => {
                    out.push_str(to);
                    rest = &rest[from.len()..];
                }
                None => {
                    out.push(ch);
                    rest = &rest[ch.len_utf8()..];
                }
            }
        }
        out
    }

    /// Applies the rules to the text lines of SRT content only. Indices,
    /// timing lines and blank lines are kept byte for byte, so a numeric key
    /// cannot corrupt `00:12:40,290`.
    pub fn apply_to_srt(&self, srt: &str) -> String {
        if self.is_empty() {
            return srt.to_string();
        }
        let mut in_text = false;
        srt.split_inclusive('\n')
            .map(|line| {
                if line.trim().is_empty() {
                    in_text = false;
                } else if line.contains("-->") {
                    in_text = true;
                } else if in_text {
                    return self.apply(line);
                }
                line.to_string()
            })
            .collect()
    }
}

fn strip_nul(s: &str) -> String {
    s.chars().filter(|c| *c != '\0').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(pairs: &[(&str, &str)]) -> Replacements {
        let map = pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        Replacements::new(&ReplacementMap(map)).unwrap()
    }

    #[test]
    fn test_apply_fixes_field_report_names() {
        let r = rules(&[
            ("Not.cafeta", "knot.kz"),
            ("Chrome Divac", "chrome-debug"),
            ("Raúl Togar", "Raúl Tovar"),
        ]);
        assert_eq!(
            r.apply("Soy Raúl Togar y uso Not.cafeta con Chrome Divac."),
            "Soy Raúl Tovar y uso knot.kz con chrome-debug."
        );
    }

    #[test]
    fn test_apply_is_case_sensitive() {
        let r = rules(&[("Knot", "knot")]);
        assert_eq!(r.apply("Knot y KNOT y knot"), "knot y KNOT y knot");
    }

    #[test]
    fn test_apply_prefers_the_longest_key() {
        let r = rules(&[("web", "Web"), ("web mcp", "WebMCP")]);
        assert_eq!(r.apply("la web mcp y la web"), "la WebMCP y la Web");
    }

    #[test]
    fn test_apply_does_not_rematch_replaced_text() {
        let r = rules(&[("a", "ab"), ("b", "c")]);
        assert_eq!(r.apply("ab"), "abc");
    }

    #[test]
    fn test_apply_handles_multibyte_text() {
        let r = rules(&[("ñu", "gnu")]);
        assert_eq!(r.apply("¿Un ñu? Sí, ñu."), "¿Un gnu? Sí, gnu.");
    }

    #[test]
    fn test_apply_to_srt_leaves_indices_and_timestamps_untouched() {
        let r = rules(&[("0", "zero"), ("1", "one")]);
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nUp 10 points\nand 1 more\n\n\
                   10\n00:00:10,000 --> 00:00:11,000\n0 left\n";
        assert_eq!(
            r.apply_to_srt(srt),
            "1\n00:00:01,000 --> 00:00:02,000\nUp onezero points\nand one more\n\n\
             10\n00:00:10,000 --> 00:00:11,000\nzero left\n"
        );
    }

    #[test]
    fn test_empty_rules_leave_text_identical() {
        let r = Replacements::default();
        assert_eq!(r.apply("abc"), "abc");
        assert_eq!(
            r.apply_to_srt("1\r\nx --> y\r\nabc\r\n"),
            "1\r\nx --> y\r\nabc\r\n"
        );
    }

    #[test]
    fn test_new_strips_nul_and_rejects_empty_keys() {
        let r = rules(&[("a\0b", "c\0d")]);
        assert_eq!(r.apply("ab"), "cd");
        let map = ReplacementMap([("\0".to_string(), "x".to_string())].into());
        assert!(Replacements::new(&map).is_err());
    }

    #[test]
    fn test_new_rejects_too_many_rules() {
        let map = (0..=MAX_REPLACEMENTS)
            .map(|i| (format!("k{i}"), "v".to_string()))
            .collect();
        let err = Replacements::new(&ReplacementMap(map)).unwrap_err();
        assert!(err.to_string().contains("Too many replacements"));
    }

    #[test]
    fn test_replacement_map_deserializes_from_an_object() {
        let map: ReplacementMap =
            serde_json::from_value(serde_json::json!({ "Not.cafeta": "knot.kz" })).unwrap();
        assert_eq!(map.0.get("Not.cafeta").map(String::as_str), Some("knot.kz"));
        assert_eq!(ReplacementMap::json_schema()["type"], "object");
    }
}
