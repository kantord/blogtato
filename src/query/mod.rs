mod grammar;
pub(crate) mod resolve;

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use anyhow::{bail, ensure};
use chrono::{DateTime, Utc};
use chumsky::prelude::*;

use crate::data::schema::FeedItem;
use grammar::{Token, arg_parser};

#[derive(Clone, Debug)]
pub(crate) struct QueryDate {
    raw: String,
    pub resolved: DateTime<Utc>,
}

impl fmt::Display for QueryDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl FromStr for QueryDate {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        grammar::date_value_parser()
            .parse(s)
            .into_result()
            .map_err(|e| anyhow::anyhow!("Invalid date: {:?}", e))
    }
}

impl std::ops::Sub for QueryDate {
    type Output = chrono::Duration;
    fn sub(self, rhs: QueryDate) -> chrono::Duration {
        self.resolved - rhs.resolved
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DateFilter {
    pub since: Option<QueryDate>,
    pub until: Option<QueryDate>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GroupKey {
    Date,
    Week,
    Feed,
}

impl GroupKey {
    pub(crate) fn extract(&self, item: &FeedItem, feed_labels: &HashMap<String, String>) -> String {
        match self {
            GroupKey::Date => item
                .date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            GroupKey::Week => item
                .date
                .map(|d| d.format("%G-W%V").to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            GroupKey::Feed => feed_labels
                .get(&item.feed)
                .cloned()
                .unwrap_or_else(|| item.feed.clone()),
        }
    }

    pub(crate) fn compare(
        &self,
        a: &FeedItem,
        b: &FeedItem,
        feed_labels: &HashMap<String, String>,
    ) -> std::cmp::Ordering {
        match self {
            GroupKey::Date | GroupKey::Week => b.date.cmp(&a.date),
            GroupKey::Feed => {
                let la = feed_labels.get(&a.feed).map_or(&a.feed, |s| s);
                let lb = feed_labels.get(&b.feed).map_or(&b.feed, |s| s);
                la.cmp(lb)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ReadFilter {
    /// No explicit filter provided (parser default)
    Any,
    /// User explicitly requested all posts via `.all`
    All,
    Read,
    Unread,
}

#[derive(Clone, Debug)]
pub(crate) struct Query {
    pub keys: Vec<GroupKey>,
    pub filter: Option<String>,
    pub id_filter: Option<String>,
    pub date_filter: DateFilter,
    pub shorthands: Vec<String>,
    pub read_filter: ReadFilter,
}

pub(crate) const DEFAULT_QUERY: &str = ".unread 90d.. /w";

pub(crate) fn parse_query_str(input: &str) -> anyhow::Result<Query> {
    let args: Vec<String> = input.split_whitespace().map(String::from).collect();
    parse_query(&args)
}

pub(crate) fn parse_query(args: &[String]) -> anyhow::Result<Query> {
    let mut keys = Vec::new();
    let mut filter = None;
    let mut since = None;
    let mut until = None;
    let mut shorthands = Vec::new();
    let mut read_filter = ReadFilter::Any;
    let mut id_filter = None;

    let parser = arg_parser();

    for arg in args {
        match parser.parse(arg).into_result() {
            Ok(token) => match token {
                Token::Group(key) => {
                    ensure!(keys.len() < 2, "Too many grouping arguments (max 2).");
                    keys.push(key);
                }
                Token::FeedFilter(s) => {
                    filter = Some(s);
                }
                Token::Range(s, u) => {
                    if let Some(s) = s {
                        since = Some(s);
                    }
                    if let Some(u) = u {
                        until = Some(u);
                    }
                }
                Token::Shorthand(s) => {
                    shorthands.push(s);
                }
                Token::ReadStatus(rf) => {
                    read_filter = rf;
                }
                Token::IdFilter(id) => {
                    id_filter = Some(id);
                }
            },
            Err(errs) => {
                let messages: Vec<String> = errs.into_iter().map(|e| e.to_string()).collect();
                bail!(
                    "Failed to parse argument '{arg}': {}\nRun 'blog --help' for query syntax.",
                    messages.join(", ")
                );
            }
        }
    }

    if let (Some(s), Some(u)) = (since.as_ref(), until.as_ref()) {
        ensure!(
            s.resolved <= u.resolved,
            "Invalid date range: {} is after {}. \
             The start date must be before the end date, e.g. 3w..1w instead of 1w..3w.",
            s.resolved.format("%Y-%m-%d"),
            u.resolved.format("%Y-%m-%d"),
        );
    }

    Ok(Query {
        keys,
        filter,
        id_filter,
        date_filter: DateFilter { since, until },
        shorthands,
        read_filter,
    })
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        match self.read_filter {
            ReadFilter::Unread => parts.push(".unread".to_string()),
            ReadFilter::Read => parts.push(".read".to_string()),
            ReadFilter::All => parts.push(".all".to_string()),
            ReadFilter::Any => {}
        }
        if let Some(ref feed) = self.filter {
            parts.push(format!("@{feed}"));
        }
        match (&self.date_filter.since, &self.date_filter.until) {
            (Some(s), Some(u)) => parts.push(format!("{s}..{u}")),
            (Some(s), None) => parts.push(format!("{s}..")),
            (None, Some(u)) => parts.push(format!("..{u}")),
            (None, None) => {}
        }
        for key in &self.keys {
            parts.push(match key {
                GroupKey::Date => "/d".to_string(),
                GroupKey::Week => "/w".to_string(),
                GroupKey::Feed => "/f".to_string(),
            });
        }
        write!(f, "{}", parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::date::start_of_day;
    use grammar::date_value_parser;
    use rstest::rstest;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[rstest]
    #[case::date("/d", GroupKey::Date)]
    #[case::week("/w", GroupKey::Week)]
    #[case::feed("/f", GroupKey::Feed)]
    fn test_parse_group_arg(#[case] input: &str, #[case] expected: GroupKey) {
        let q = parse_query(&args(&[input])).unwrap();
        assert_eq!(q.keys, vec![expected]);
    }

    #[test]
    fn test_parse_group_arg_invalid() {
        let result = parse_query(&args(&["/x"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Failed to parse argument"), "got: {msg}");
    }

    #[test]
    fn test_too_many_groups() {
        let result = parse_query(&args(&["/d", "/f", "/w"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Too many grouping arguments"), "got: {msg}");
    }

    #[test]
    fn test_bare_word_is_shorthand() {
        let q = parse_query(&args(&["d"])).unwrap();
        assert_eq!(q.shorthands, vec!["d".to_string()]);
    }

    #[test]
    fn test_multiple_shorthands() {
        let q = parse_query(&args(&["a", "/d", "@hn"])).unwrap();
        assert_eq!(q.shorthands, vec!["a".to_string()]);
        assert_eq!(q.keys, vec![GroupKey::Date]);
        assert_eq!(q.filter, Some("hn".to_string()));
    }

    #[test]
    fn test_feed_filter() {
        let q = parse_query(&args(&["@myblog"])).unwrap();
        assert_eq!(q.filter, Some("myblog".to_string()));
    }

    fn parse_date(value: &str) -> anyhow::Result<QueryDate> {
        let parser = date_value_parser();
        parser
            .parse(value)
            .into_result()
            .map_err(|e| anyhow::anyhow!("Parse error: {:?}", e))
    }

    #[test]
    fn test_parse_date_value_absolute_date() {
        let dt = parse_date("2024-01-15").unwrap();
        assert_eq!(dt.resolved.format("%Y-%m-%d").to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_value_another_absolute_date() {
        let dt = parse_date("2024-04-01").unwrap();
        assert_eq!(dt.resolved.format("%Y-%m-%d").to_string(), "2024-04-01");
    }

    #[rstest]
    #[case::one_week("1w", 6, 8)]
    #[case::two_days("2d", 1, 3)]
    #[case::three_months("3months", 85, 95)]
    #[case::three_months_short("3m", 85, 95)]
    fn test_parse_date_relative(#[case] input: &str, #[case] min_days: i64, #[case] max_days: i64) {
        let dt = parse_date(input).unwrap();
        let diff_days = (Utc::now() - dt.resolved).num_days();
        assert!(
            (min_days..=max_days).contains(&diff_days),
            "{input} should be ~{min_days}-{max_days} days ago, got {diff_days} days"
        );
    }

    #[test]
    fn test_parse_date_value_yesterday() {
        let dt = parse_date("yesterday").unwrap();
        let expected = start_of_day((Utc::now() - chrono::Duration::days(1)).date_naive());
        assert_eq!(
            dt.resolved.format("%Y-%m-%d").to_string(),
            expected.format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn test_parse_date_value_today() {
        let dt = parse_date("today").unwrap();
        let expected = start_of_day(Utc::now().date_naive());
        assert_eq!(
            dt.resolved.format("%Y-%m-%d").to_string(),
            expected.format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn test_parse_date_value_nonsense_returns_error() {
        assert!(parse_date("nonsense").is_err());
    }

    #[test]
    fn test_combined_args() {
        let q = parse_query(&args(&["/d", "@blog", "2024-01-15.."])).unwrap();
        assert_eq!(q.keys, vec![GroupKey::Date]);
        assert_eq!(q.filter, Some("blog".to_string()));
        assert!(q.date_filter.since.is_some());
    }

    #[test]
    fn test_range_both() {
        let q = parse_query(&args(&["2024-01-15..2024-02-01"])).unwrap();
        assert_eq!(
            q.date_filter
                .since
                .unwrap()
                .resolved
                .format("%Y-%m-%d")
                .to_string(),
            "2024-01-15"
        );
        assert_eq!(
            q.date_filter
                .until
                .unwrap()
                .resolved
                .format("%Y-%m-%d")
                .to_string(),
            "2024-02-01"
        );
    }

    #[test]
    fn test_range_open_end() {
        let q = parse_query(&args(&["2024-01-15.."])).unwrap();
        assert_eq!(
            q.date_filter
                .since
                .unwrap()
                .resolved
                .format("%Y-%m-%d")
                .to_string(),
            "2024-01-15"
        );
        assert!(q.date_filter.until.is_none());
    }

    #[test]
    fn test_range_open_start() {
        let q = parse_query(&args(&["..2024-02-01"])).unwrap();
        assert!(q.date_filter.since.is_none());
        assert_eq!(
            q.date_filter
                .until
                .unwrap()
                .resolved
                .format("%Y-%m-%d")
                .to_string(),
            "2024-02-01"
        );
    }

    #[test]
    fn test_range_relative() {
        let q = parse_query(&args(&["3w..1w"])).unwrap();
        assert!(q.date_filter.since.is_some());
        assert!(q.date_filter.until.is_some());
        assert!(q.date_filter.since.unwrap().resolved < q.date_filter.until.unwrap().resolved);
    }

    #[test]
    fn test_inverted_date_range_errors() {
        let result = parse_query(&args(&["1w..3w"]));
        assert!(result.is_err(), "inverted range 1w..3w should fail");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Invalid date range"),
            "error should mention invalid date range, got: {msg}"
        );
    }

    #[test]
    fn test_read_filter() {
        let q = parse_query(&args(&[".read"])).unwrap();
        assert_eq!(q.read_filter, ReadFilter::Read);
    }

    #[test]
    fn test_unread_filter() {
        let q = parse_query(&args(&[".unread"])).unwrap();
        assert_eq!(q.read_filter, ReadFilter::Unread);
    }

    #[test]
    fn test_default_read_filter_is_any() {
        let q = parse_query(&args(&["/d"])).unwrap();
        assert_eq!(q.read_filter, ReadFilter::Any);
    }

    #[test]
    fn test_all_filter() {
        let q = parse_query(&args(&[".all"])).unwrap();
        assert_eq!(q.read_filter, ReadFilter::All);
    }

    #[test]
    fn test_read_filter_combined() {
        let q = parse_query(&args(&[".unread", "@hn", "/d"])).unwrap();
        assert_eq!(q.read_filter, ReadFilter::Unread);
        assert_eq!(q.filter, Some("hn".to_string()));
        assert_eq!(q.keys, vec![GroupKey::Date]);
    }

    #[test]
    fn test_id_filter() {
        let q = parse_query(&args(&["id:abc123"])).unwrap();
        assert_eq!(q.id_filter, Some("abc123".to_string()));
    }

    #[test]
    fn test_id_filter_combined() {
        let q = parse_query(&args(&["id:abc123", ".all"])).unwrap();
        assert_eq!(q.id_filter, Some("abc123".to_string()));
        assert_eq!(q.read_filter, ReadFilter::All);
    }

    // ── QueryDate Display tests ──────────────────────────────────────────────

    /// QueryDate::Display must echo back the exact raw text, not a resolved date.
    #[rstest]
    #[case("90d")]
    #[case("2w")]
    #[case("today")]
    #[case("yesterday")]
    #[case("2024-01-15")]
    #[case("3months")]
    fn test_query_date_display_preserves_raw_text(#[case] input: &str) {
        let qd: QueryDate = input.parse().expect("should parse as QueryDate");
        assert_eq!(qd.to_string(), input);
    }

    // ── Query Display: read filter ───────────────────────────────────────────

    #[test]
    fn test_display_read_filter_unread() {
        let q = parse_query_str(".unread").unwrap();
        assert!(q.to_string().contains(".unread"), "got: {}", q);
    }

    #[test]
    fn test_display_read_filter_read() {
        let q = parse_query_str(".read").unwrap();
        assert!(q.to_string().contains(".read"), "got: {}", q);
    }

    #[test]
    fn test_display_read_filter_all() {
        let q = parse_query_str(".all").unwrap();
        assert!(q.to_string().contains(".all"), "got: {}", q);
    }

    #[test]
    fn test_display_read_filter_any_omitted() {
        // ReadFilter::Any is the default — it must NOT appear in the output.
        let q = parse_query_str("/d").unwrap();
        let s = q.to_string();
        assert!(
            !s.contains(".read") && !s.contains(".unread") && !s.contains(".all"),
            "Any read-filter should be omitted, got: {s}"
        );
    }

    // ── Query Display: grouping keys ─────────────────────────────────────────

    #[rstest]
    #[case("/d")]
    #[case("/w")]
    #[case("/f")]
    fn test_display_single_group_key(#[case] input: &str) {
        let q = parse_query_str(input).unwrap();
        assert!(
            q.to_string().contains(input),
            "expected {input} in output, got: {}",
            q
        );
    }

    #[test]
    fn test_display_multiple_group_keys() {
        let q = parse_query_str("/d /f").unwrap();
        let s = q.to_string();
        assert!(s.contains("/d"), "expected /d in output, got: {s}");
        assert!(s.contains("/f"), "expected /f in output, got: {s}");
    }

    // ── Query Display: feed filter ───────────────────────────────────────────

    #[test]
    fn test_display_feed_filter() {
        let q = parse_query_str("@myblog").unwrap();
        assert!(q.to_string().contains("@myblog"), "got: {}", q);
    }

    // ── Query Display: date ranges ───────────────────────────────────────────

    #[test]
    fn test_display_date_range_both() {
        let q = parse_query_str("2024-01-15..2024-02-01").unwrap();
        let s = q.to_string();
        assert!(
            s.contains("2024-01-15..2024-02-01"),
            "expected full range in output, got: {s}"
        );
    }

    #[test]
    fn test_display_date_range_open_end() {
        let q = parse_query_str("2024-01-15..").unwrap();
        let s = q.to_string();
        assert!(
            s.contains("2024-01-15.."),
            "expected open-ended since range in output, got: {s}"
        );
    }

    #[test]
    fn test_display_date_range_open_start() {
        let q = parse_query_str("..2024-02-01").unwrap();
        let s = q.to_string();
        assert!(
            s.contains("..2024-02-01"),
            "expected open-start until range in output, got: {s}"
        );
    }

    #[test]
    fn test_display_relative_date_range_raw_text() {
        // Relative dates must appear as their raw strings, not resolved timestamps.
        let q = parse_query_str("90d..").unwrap();
        let s = q.to_string();
        assert!(
            s.contains("90d.."),
            "expected raw relative date in output, got: {s}"
        );
    }

    // ── Query Display: round-trip ────────────────────────────────────────────

    /// parse → display → parse must yield an equivalent query.
    #[rstest]
    #[case(".unread /w @hn")]
    #[case(".read /d @myblog 2024-01-15..2024-02-01")]
    #[case(".all /f")]
    #[case("90d.. /w")]
    #[case("..2024-06-01")]
    fn test_display_round_trips(#[case] input: &str) {
        let q1 = parse_query_str(input).expect("first parse should succeed");
        let serialized = q1.to_string();
        let q2 = parse_query_str(&serialized).expect("re-parse of Display output should succeed");

        assert_eq!(q1.keys, q2.keys, "keys differ after round-trip");
        assert_eq!(q1.filter, q2.filter, "filter differs after round-trip");
        assert_eq!(
            q1.read_filter, q2.read_filter,
            "read_filter differs after round-trip"
        );

        // Date bounds: allow 60-second tolerance for relative dates.
        match (q1.date_filter.since, q2.date_filter.since) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                let diff = (a.resolved - b.resolved).num_seconds().abs();
                assert!(diff < 60, "since differs by {diff}s after round-trip");
            }
            _ => panic!("since presence changed after round-trip"),
        }
        match (q1.date_filter.until, q2.date_filter.until) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                let diff = (a.resolved - b.resolved).num_seconds().abs();
                assert!(diff < 60, "until differs by {diff}s after round-trip");
            }
            _ => panic!("until presence changed after round-trip"),
        }
    }
}
