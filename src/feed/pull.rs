use chrono::{DateTime, Utc};
use indicatif::ProgressBar;
use rayon::prelude::*;

use crate::data::Transaction;
use crate::data::schema::{FeedItem, FeedSource, ReadMark};
use crate::feed::FeedMeta;

pub(crate) type FetchResult = (FeedSource, Result<(FeedMeta, Vec<FeedItem>), String>);

const INITIAL_RECENT_DAYS: i64 = 60;
const INITIAL_UNREAD_CAP: usize = 5;

const FETCH_THREADS: usize = 48;

/// Fetch all feeds in parallel.
pub(crate) fn fetch_feeds(sources: &[FeedSource], pb: &ProgressBar) -> Vec<FetchResult> {
    let client = crate::utils::http::http_client();
    pb.set_length(sources.len() as u64);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(FETCH_THREADS)
        .build()
        .expect("failed to build fetch thread pool");

    pool.install(|| {
        sources
            .par_iter()
            .map(|source| {
                pb.set_message(source.url.clone());
                let result = crate::feed::fetch_source(&client, source).map_err(|e| e.to_string());
                pb.inc(1);
                (source.clone(), result)
            })
            .collect()
    })
}

/// Given a list of newly fetched posts, return the IDs of posts that should be
/// marked as read immediately (because they are "old" relative to subscribe time).
///
/// Rules:
/// - Posts older than 2 months are always marked as read
/// - Of posts within 2 months, only the 5 most recent stay unread
/// - If ALL posts are older than 2 months, the single most recent stays unread
/// - Posts with no date are treated as old
pub(crate) fn initial_read_ids(items: &[FeedItem], now: DateTime<Utc>) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }

    let two_months_ago = now - chrono::Duration::days(INITIAL_RECENT_DAYS);

    let mut recent: Vec<&FeedItem> = items
        .iter()
        .filter(|i| matches!(i.date, Some(d) if d >= two_months_ago))
        .collect();
    recent.sort_by_key(|b| std::cmp::Reverse(b.date));

    let unread_ids: std::collections::HashSet<&str> = if recent.is_empty() {
        // All posts are old or dateless — keep the single most recent unread
        let newest = items.iter().max_by_key(|i| i.date).unwrap();
        std::iter::once(newest.raw_id.as_str()).collect()
    } else {
        recent
            .iter()
            .take(INITIAL_UNREAD_CAP)
            .map(|i| i.raw_id.as_str())
            .collect()
    };

    items
        .iter()
        .filter(|i| !unread_ids.contains(i.raw_id.as_str()))
        .map(|i| i.raw_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};
    use rstest::rstest;

    fn make_item(id: &str, age_days: Option<i64>, now: DateTime<Utc>) -> FeedItem {
        FeedItem {
            title: id.to_string(),
            date: age_days.map(|d| now - Duration::days(d)),
            feed: "test-feed".to_string(),
            link: String::new(),
            raw_id: id.to_string(),
        }
    }

    // (ages_in_days, expected_unread_count)
    // None means no date on the post
    #[rstest]
    // empty feed — nothing to mark
    #[case::no_posts(vec![], 0)]
    // 3 recent posts — all stay unread (under cap of 5)
    #[case::few_recent(vec![Some(7), Some(21), Some(42)], 3)]
    // 7 recent posts — only 5 newest stay unread
    #[case::many_recent(vec![Some(7), Some(14), Some(21), Some(28), Some(35), Some(42), Some(49)], 5)]
    // 3 old posts — 1 most recent stays unread
    #[case::all_old(vec![Some(90), Some(120), Some(150)], 1)]
    // 2 recent + 3 old — 2 stay unread
    #[case::mixed_few_recent(vec![Some(7), Some(21), Some(90), Some(120), Some(150)], 2)]
    // 3 recent + 5 old — 3 stay unread
    #[case::mixed_more_recent(vec![Some(7), Some(21), Some(28), Some(90), Some(120), Some(150), Some(180), Some(210)], 3)]
    // all posts have no date — 1 stays unread (treated as old)
    #[case::no_dates(vec![None, None, None], 1)]
    // mix of no-date and recent — recent stay unread
    #[case::some_no_dates(vec![Some(7), None, None, Some(21)], 2)]
    fn test_initial_read_ids(#[case] ages: Vec<Option<i64>>, #[case] expected_unread: usize) {
        let now = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let items: Vec<FeedItem> = ages
            .iter()
            .enumerate()
            .map(|(i, age)| make_item(&format!("post-{i}"), *age, now))
            .collect();

        let read_ids = initial_read_ids(&items, now);
        let unread_count = items.len() - read_ids.len();
        assert_eq!(
            unread_count, expected_unread,
            "ages={ages:?}: expected {expected_unread} unread, got {unread_count} (read_ids={read_ids:?})"
        );
    }
}

fn apply_feed(tx: &mut Transaction, mut source: FeedSource, meta: FeedMeta, items: Vec<FeedItem>) {
    let feed_id = tx.feeds.id_of(&source);
    let now = Utc::now();

    if !source.is_fetched {
        for id in initial_read_ids(&items, now) {
            tx.reads.upsert(ReadMark {
                post_id: id,
                read_at: now,
            });
        }
    }

    for mut item in items {
        item.feed = feed_id.clone();
        tx.posts.upsert(item);
    }

    source.is_fetched = true;
    source.title = meta.title;
    source.site_url = meta.site_url;
    source.description = meta.description;
    tx.feeds.upsert(source);
}

/// Apply fetched feed results to the store.
///
/// If `ingest_filter` is set and jq fails, the entire sync is aborted rather
/// than skipping the feed. This is intentional: a broken filter would silently
/// drop all posts from every feed if we continued.
pub(crate) fn apply_fetched(
    tx: &mut Transaction,
    results: Vec<FetchResult>,
    pb: &ProgressBar,
    ingest_filter: Option<&str>,
) -> anyhow::Result<()> {
    for (source, result) in results {
        match result {
            Ok((meta, items)) => {
                let items = crate::utils::jq::map_through_jq(items, ingest_filter)?;
                apply_feed(tx, source, meta, items);
            }
            Err(e) => pb.suspend(|| eprintln!("Error fetching {}: {}", source.url, e)),
        }
    }
    Ok(())
}
