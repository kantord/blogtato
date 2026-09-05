mod group;
mod item;

use std::collections::{HashMap, HashSet};

use crate::data::index::FeedIndex;
use crate::query::GroupKey;

pub(crate) use group::render_grouped;

pub(crate) fn build_feed_labels(fi: &FeedIndex) -> HashMap<String, String> {
    fi.entries
        .iter()
        .map(|e| {
            let label = if e.feed.title.is_empty() {
                format!("@{} {}", e.shorthand, e.feed.url)
            } else {
                format!("@{} {}", e.shorthand, e.feed.title)
            };
            (e.id.clone(), label)
        })
        .collect()
}

pub(crate) struct Style {
    pub bold: &'static str,
    pub dim: &'static str,
    pub italic: &'static str,
    pub date_color: &'static str,
    pub reset: &'static str,
}

impl Style {
    pub fn new(color: bool) -> Self {
        if color {
            Self {
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                italic: "\x1b[3m",
                date_color: "\x1b[36m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                bold: "",
                dim: "",
                italic: "",
                date_color: "",
                reset: "",
            }
        }
    }
}

pub(crate) struct RenderCtx<'a> {
    pub all_keys: &'a [GroupKey],
    pub shorthands: &'a HashMap<String, String>,
    pub feed_labels: &'a HashMap<String, String>,
    pub read_ids: &'a HashSet<String>,
    pub color: bool,
    pub shorthand_width: usize,
    pub max_width: Option<usize>,
    pub compact: bool,
}

impl<'a> RenderCtx<'a> {
    pub fn shorthand_width_from(
        items: &[&crate::data::schema::FeedItem],
        shorthands: &HashMap<String, String>,
    ) -> usize {
        items
            .iter()
            .filter_map(|item| shorthands.get(&item.raw_id))
            .map(|s| s.len())
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::schema::FeedItem;
    use crate::query::{DateFilter, QueryDate};
    use crate::utils::date::start_of_day;
    use chrono::{DateTime, NaiveDate, Utc};
    use rstest::rstest;

    fn utc_date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        start_of_day(NaiveDate::from_ymd_opt(year, month, day).unwrap())
    }

    fn query_date(year: i32, month: u32, day: u32) -> QueryDate {
        format!("{year:04}-{month:02}-{day:02}").parse().unwrap()
    }

    fn feed_item(title: &str, date: &str, feed: &str) -> FeedItem {
        FeedItem {
            title: title.to_string(),
            date: Some(start_of_day(
                NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            )),
            feed: feed.to_string(),
            link: String::new(),
            raw_id: String::new(),
        }
    }

    fn feed_item_with_raw_id(title: &str, date: &str, feed: &str, raw_id: &str) -> FeedItem {
        FeedItem {
            raw_id: raw_id.to_string(),
            ..feed_item(title, date, feed)
        }
    }

    fn no_labels() -> &'static HashMap<String, String> {
        use std::sync::LazyLock;
        static EMPTY: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);
        &EMPTY
    }

    fn no_reads() -> &'static HashSet<String> {
        use std::sync::LazyLock;
        static EMPTY: LazyLock<HashSet<String>> = LazyLock::new(HashSet::new);
        &EMPTY
    }

    fn default_ctx<'a>(
        keys: &'a [GroupKey],
        shorthands: &'a HashMap<String, String>,
        feed_labels: &'a HashMap<String, String>,
        read_ids: &'a HashSet<String>,
        max_width: Option<usize>,
        items: &[&FeedItem],
    ) -> RenderCtx<'a> {
        RenderCtx {
            all_keys: keys,
            shorthand_width: RenderCtx::shorthand_width_from(items, shorthands),
            shorthands,
            feed_labels,
            read_ids,
            color: false,
            max_width,
            compact: false,
        }
    }

    #[rstest]
    #[case::unread_no_grouping(&[], false, "* 2024-01-15  abc Post (Alice)")]
    #[case::unread_grouped_by_date(&[GroupKey::Date], false, "* abc Post (Alice)")]
    #[case::unread_grouped_by_feed(&[GroupKey::Feed], false, "* 2024-01-15  abc Post")]
    #[case::unread_grouped_by_both(&[GroupKey::Date, GroupKey::Feed], false, "* abc Post")]
    #[case::read_no_grouping(&[], true, "  2024-01-15  abc Post (Alice)")]
    #[case::read_grouped_by_date(&[GroupKey::Date], true, "  abc Post (Alice)")]
    #[case::read_grouped_by_feed(&[GroupKey::Feed], true, "  2024-01-15  abc Post")]
    #[case::read_grouped_by_both(&[GroupKey::Date, GroupKey::Feed], true, "  abc Post")]
    fn test_format_item_read_marker(
        #[case] keys: &[GroupKey],
        #[case] is_read: bool,
        #[case] expected: &str,
    ) {
        let i = feed_item("Post", "2024-01-15", "Alice");
        let mut shorthands = HashMap::new();
        shorthands.insert(i.raw_id.clone(), "abc".to_string());
        let mut read_ids = HashSet::new();
        if is_read {
            read_ids.insert(i.raw_id.clone());
        }
        let ctx = RenderCtx {
            all_keys: keys,
            shorthands: &shorthands,
            feed_labels: no_labels(),
            read_ids: &read_ids,
            color: false,
            shorthand_width: 3,
            max_width: None,
            compact: false,
        };
        assert_eq!(item::format_item(&i, None, &ctx), expected);
    }

    #[test]
    fn test_format_date_with_date() {
        let i = feed_item("Post", "2024-01-15", "Alice");
        assert_eq!(item::format_date(&i), "2024-01-15");
    }

    #[test]
    fn test_format_date_without_date() {
        let i = FeedItem {
            date: None,
            ..feed_item("Post", "2024-01-01", "Alice")
        };
        assert_eq!(item::format_date(&i), "unknown");
    }

    #[test]
    fn test_render_flat() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-01", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let ctx = default_ctx(&[], no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "* 2024-01-02   Post A (Alice)\n* 2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_flat_with_read_marks() {
        let items = [
            feed_item_with_raw_id("Post A", "2024-01-02", "Alice", "id-a"),
            feed_item_with_raw_id("Post B", "2024-01-01", "Bob", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids: HashSet<String> = ["id-a".to_string()].into();

        let ctx = default_ctx(&[], no_labels(), no_labels(), &read_ids, None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "  2024-01-02   Post A (Alice)\n* 2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_grouped_with_mixed_read_status() {
        let items = [
            feed_item_with_raw_id("Post A", "2024-01-02", "Alice", "id-a"),
            feed_item_with_raw_id("Post B", "2024-01-02", "Bob", "id-b"),
            feed_item_with_raw_id("Post C", "2024-01-01", "Alice", "id-c"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let read_ids: HashSet<String> = ["id-b".to_string()].into();

        let keys = [GroupKey::Date];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), &read_ids, None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  *  Post A (Alice)
     Post B (Bob)


=== 2024-01-01 ===

  *  Post C (Alice)


"
        );
    }

    #[test]
    fn test_render_grouped_by_date() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-02", "Bob"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Date];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  *  Post A (Alice)
  *  Post B (Bob)


=== 2024-01-01 ===

  *  Post C (Alice)


"
        );
    }

    #[test]
    fn test_render_grouped_by_feed() {
        let items = [
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Feed];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "\
=== Alice ===

  * 2024-01-01   Post B


=== Bob ===

  * 2024-01-02   Post A


"
        );
    }

    #[test]
    fn test_render_grouped_by_date_then_feed() {
        let items = [
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-02", "Alice"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Date, GroupKey::Feed];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "\
=== 2024-01-02 ===

  --- Alice ---
    *  Post B

  --- Bob ---
    *  Post A



=== 2024-01-01 ===

  --- Alice ---
    *  Post C



"
        );
    }

    #[test]
    fn test_render_grouped_by_feed_then_date() {
        let items = [
            feed_item("Post A", "2024-01-02", "Bob"),
            feed_item("Post B", "2024-01-02", "Alice"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Feed, GroupKey::Date];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(
            output,
            "\
=== Alice ===

  --- 2024-01-02 ---
    *  Post B

  --- 2024-01-01 ---
    *  Post C



=== Bob ===

  --- 2024-01-02 ---
    *  Post A



"
        );
    }

    #[test]
    fn test_render_empty_items() {
        let refs: Vec<&FeedItem> = vec![];
        let keys = [GroupKey::Date];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);

        assert_eq!(render_grouped(&refs, &ctx), "");
    }

    #[test]
    fn test_date_ordering_is_descending() {
        let items = [
            feed_item("Old", "2024-01-01", "Alice"),
            feed_item("New", "2024-01-03", "Alice"),
            feed_item("Mid", "2024-01-02", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Date];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec![
                "=== 2024-01-03 ===",
                "=== 2024-01-02 ===",
                "=== 2024-01-01 ==="
            ]
        );
    }

    #[test]
    fn test_feed_ordering_is_ascending() {
        let items = [
            feed_item("Post", "2024-01-01", "Charlie"),
            feed_item("Post", "2024-01-02", "Alice"),
            feed_item("Post", "2024-01-03", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Feed];
        let ctx = default_ctx(&keys, no_labels(), no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        let headers: Vec<&str> = output.lines().filter(|l| l.starts_with("===")).collect();
        assert_eq!(
            headers,
            vec!["=== Alice ===", "=== Bob ===", "=== Charlie ==="]
        );
    }

    #[test]
    fn test_render_grouped_with_shorthands() {
        let items = [feed_item_with_raw_id(
            "Post A",
            "2024-01-02",
            "Alice",
            "id-a",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id-a".to_string(), "sDf".to_string());
        let ctx = default_ctx(&[], &shorthands, no_labels(), no_reads(), None, &refs);
        let output = render_grouped(&refs, &ctx);
        assert_eq!(output, "* 2024-01-02  sDf Post A (Alice)\n");
    }

    #[test]
    fn test_cjk_characters_respect_display_width() {
        use unicode_width::UnicodeWidthStr;

        let cjk_title = "你好世界测试标题很长";
        let items = [feed_item_with_raw_id(
            cjk_title,
            "2024-01-15",
            "feed1",
            "id1",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id1".to_string(), "a".to_string());
        let mut labels = HashMap::new();
        labels.insert("feed1".to_string(), "@x Blog".to_string());

        let max_width = 40;
        let ctx = default_ctx(
            &[],
            &shorthands,
            &labels,
            no_reads(),
            Some(max_width),
            &refs,
        );
        let output = render_grouped(&refs, &ctx);

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let width = line.width();
            assert!(
                width <= max_width,
                "line display width ({width}) exceeds max_width ({max_width}): {line}"
            );
        }
    }

    #[test]
    fn test_long_lines_are_truncated_to_max_width() {
        use unicode_width::UnicodeWidthStr;

        let long_title =
            "An extremely long post title that should definitely be truncated to fit the width";
        let items = [feed_item_with_raw_id(
            long_title,
            "2024-01-15",
            "feed1",
            "id1",
        )];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let mut shorthands = HashMap::new();
        shorthands.insert("id1".to_string(), "a".to_string());
        let mut labels = HashMap::new();
        labels.insert(
            "feed1".to_string(),
            "@x A Fairly Long Blog Name".to_string(),
        );

        let max_width = 60;
        let ctx = default_ctx(
            &[],
            &shorthands,
            &labels,
            no_reads(),
            Some(max_width),
            &refs,
        );
        let output = render_grouped(&refs, &ctx);

        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let width = line.width();
            assert!(
                width <= max_width,
                "line exceeds {max_width} columns ({width} chars): {line}",
            );
            assert!(
                line.contains('\u{2026}'),
                "truncated line should contain \u{2026}: {line}"
            );
        }
    }

    fn filter_items(items: &[FeedItem], date_filter: &DateFilter) -> Vec<String> {
        let filtered: Vec<&FeedItem> = items
            .iter()
            .filter(|item| {
                if let Some(ref since) = date_filter.since {
                    match item.date {
                        Some(d) if d < since.resolved => return false,
                        None => return false,
                        _ => {}
                    }
                }
                if let Some(ref until) = date_filter.until {
                    match item.date {
                        Some(d) if d > until.resolved => return false,
                        None => return false,
                        _ => {}
                    }
                }
                true
            })
            .collect();
        let ctx = default_ctx(&[], no_labels(), no_labels(), no_reads(), None, &filtered);
        let output = render_grouped(&filtered, &ctx);
        output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect()
    }

    #[rstest]
    #[case::since_filters_old(Some(query_date(2024, 1, 15)), None, &["Mid Post", "New Post"], &["Old Post"])]
    #[case::until_filters_new(None, Some(query_date(2024, 1, 15)), &["Old Post", "Mid Post"], &["New Post"])]
    #[case::since_and_until(Some(query_date(2024, 1, 10)), Some(query_date(2024, 1, 20)), &["Mid Post"], &["Old Post", "New Post"])]
    fn test_date_filter(
        #[case] since: Option<QueryDate>,
        #[case] until: Option<QueryDate>,
        #[case] present: &[&str],
        #[case] absent: &[&str],
    ) {
        let items = [
            feed_item("Old Post", "2024-01-01", "Alice"),
            feed_item("Mid Post", "2024-01-15", "Alice"),
            feed_item("New Post", "2024-02-01", "Alice"),
        ];
        let df = DateFilter { since, until };
        let lines = filter_items(&items, &df);
        for title in present {
            assert!(
                lines.iter().any(|l| l.contains(title)),
                "{title} should be included"
            );
        }
        for title in absent {
            assert!(
                !lines.iter().any(|l| l.contains(title)),
                "{title} should be filtered out"
            );
        }
    }

    #[rstest]
    #[case::since_includes_boundary(Some(query_date(2024, 1, 15)), None, &["Exact"], &["Before"])]
    #[case::until_includes_boundary(None, Some(query_date(2024, 1, 15)), &["Exact"], &["After"])]
    fn test_boundary_inclusion(
        #[case] since: Option<QueryDate>,
        #[case] until: Option<QueryDate>,
        #[case] present: &[&str],
        #[case] absent: &[&str],
    ) {
        let items = [
            feed_item("Before", "2024-01-14", "Alice"),
            feed_item("Exact", "2024-01-15", "Alice"),
            feed_item("After", "2024-01-16", "Alice"),
        ];
        let df = DateFilter { since, until };
        let lines = filter_items(&items, &df);
        for title in present {
            assert!(
                lines.iter().any(|l| l.contains(title)),
                "{title} should be included"
            );
        }
        for title in absent {
            assert!(
                !lines.iter().any(|l| l.contains(title)),
                "{title} should be filtered out"
            );
        }
    }

    #[test]
    fn test_render_grouped_compact() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-02", "Bob"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Date];
        let ctx = RenderCtx {
            all_keys: &keys,
            shorthand_width: RenderCtx::shorthand_width_from(&refs, &HashMap::new()),
            shorthands: no_labels(),
            feed_labels: no_labels(),
            read_ids: no_reads(),
            color: false,
            max_width: None,
            compact: true,
        };
        let output = render_grouped(&refs, &ctx);
        // Compact: no blank line after header, no blank lines between groups
        assert_eq!(
            output,
            "=== 2024-01-02 ===\n  *  Post A (Alice)\n  *  Post B (Bob)\n=== 2024-01-01 ===\n  *  Post C (Alice)\n"
        );
    }

    #[test]
    fn test_render_grouped_compact_flat_unchanged() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-01", "Bob"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let ctx = RenderCtx {
            all_keys: &[],
            shorthand_width: RenderCtx::shorthand_width_from(&refs, &HashMap::new()),
            shorthands: no_labels(),
            feed_labels: no_labels(),
            read_ids: no_reads(),
            color: false,
            max_width: None,
            compact: true,
        };
        let output = render_grouped(&refs, &ctx);
        // Flat output is identical whether compact or not
        assert_eq!(
            output,
            "* 2024-01-02   Post A (Alice)\n* 2024-01-01   Post B (Bob)\n"
        );
    }

    #[test]
    fn test_render_grouped_compact_nested() {
        let items = [
            feed_item("Post A", "2024-01-02", "Alice"),
            feed_item("Post B", "2024-01-02", "Bob"),
            feed_item("Post C", "2024-01-01", "Alice"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();

        let keys = [GroupKey::Date, GroupKey::Feed];
        let ctx = RenderCtx {
            all_keys: &keys,
            shorthand_width: RenderCtx::shorthand_width_from(&refs, &HashMap::new()),
            shorthands: no_labels(),
            feed_labels: no_labels(),
            read_ids: no_reads(),
            color: false,
            max_width: None,
            compact: true,
        };
        let output = render_grouped(&refs, &ctx);
        // Top-level has no extra blank lines; nested still has separator
        assert_eq!(
            output,
            "=== 2024-01-02 ===\n  --- Alice ---\n    *  Post A\n  --- Bob ---\n    *  Post B\n=== 2024-01-01 ===\n  --- Alice ---\n    *  Post C\n"
        );
    }
}
