use std::collections::HashSet;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use synctato::{SyncEvent, SyncResult};

use crate::data::BlogData;
use crate::data::index::{FeedIndex, feed_index};
use crate::data::schema::FeedSource;
use crate::utils::progress::spinner;
use crate::utils::version_check::check_for_newer_version;

use crate::feed::pull::{apply_fetched, fetch_feeds};

#[cfg(test)]
use crate::data::index::FeedEntry;

const CRATES_IO_URL: &str = "https://crates.io/api/v1/crates/blogtato";

fn do_sync_remote(store: &mut BlogData) -> anyhow::Result<SyncResult> {
    let mut sp: Option<ProgressBar> = None;
    store.sync_remote(|event| match event {
        SyncEvent::Fetching => {
            sp = Some(spinner("Fetching..."));
        }
        SyncEvent::FetchDone => {
            if let Some(s) = sp.take() {
                s.finish_with_message("Fetching... done.");
            }
        }
        SyncEvent::Pushing { first_push } => {
            let msg = if first_push {
                "Pushing to remote (first sync)..."
            } else {
                "Pushing..."
            };
            sp = Some(spinner(msg));
        }
        SyncEvent::PushDone { first_push } => {
            let msg = if first_push {
                "Pushing to remote (first sync)... done."
            } else {
                "Pushing... done."
            };
            if let Some(s) = sp.take() {
                s.finish_with_message(msg);
            }
        }
        SyncEvent::MergingRemote => {
            sp = Some(spinner("Merging remote data..."));
        }
        SyncEvent::MergeDone { counts } => {
            if let Some(s) = sp.take() {
                let detail: Vec<String> = counts
                    .iter()
                    .map(|(name, count)| format!("{} {}", count, name))
                    .collect();
                s.finish_with_message(format!(
                    "Merging remote data... done ({} from remote).",
                    detail.join(", ")
                ));
            }
        }
    })
}

fn resolve_sync_sources(
    feed_index: &FeedIndex,
    selectors: &[String],
) -> anyhow::Result<Vec<FeedSource>> {
    if selectors.is_empty() {
        return Ok(feed_index
            .entries
            .iter()
            .map(|entry| entry.feed.clone())
            .collect());
    }

    let mut seen = HashSet::new();
    let mut resolved = Vec::new();

    for selector in selectors {
        let shorthand = selector.strip_prefix('@').ok_or_else(|| {
            anyhow::anyhow!("Invalid feed selector: {} (expected @shorthand)", selector)
        })?;
        let entry = feed_index
            .entries
            .iter()
            .find(|entry| entry.shorthand == shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{}", shorthand))?;
        if seen.insert(entry.feed.url.clone()) {
            resolved.push(entry.feed.clone());
        }
    }

    Ok(resolved)
}

pub(crate) fn cmd_sync(
    store: &mut BlogData,
    selectors: &[String],
    no_remote: bool,
) -> anyhow::Result<()> {
    let needs_push = if !no_remote {
        // Sync with remote first so we discover feeds added on other devices
        let result = do_sync_remote(store)?;
        match result {
            SyncResult::NoRemote => {
                eprintln!(
                    "warning: no remote configured; run `blog git remote add origin <url>` to enable sync"
                );
                false
            }
            SyncResult::NoGitRepo => false,
            SyncResult::Synced | SyncResult::AlreadyUpToDate => true,
        }
    } else {
        false
    };

    let fi = feed_index(store.feeds());
    let sources = resolve_sync_sources(&fi, selectors)?;

    // Fetch feeds outside the transaction (network I/O, no lock held)
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} Pulling feeds [{bar:20.cyan/dim}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = fetch_feeds(&sources, &pb);
    pb.finish_and_clear();

    // Apply results inside a locked transaction
    let ingest_filter = crate::data::get_config_value(store, "ingest_filter");
    store.transact("pull feeds", |tx| {
        apply_fetched(tx, results, &pb, ingest_filter.as_deref())
    })?;

    // Sync again to push the freshly fetched feed data back to remote
    if needs_push {
        let push_result = do_sync_remote(store)?;
        match push_result {
            SyncResult::Synced => {} // pushed successfully, spinners already shown
            SyncResult::AlreadyUpToDate => {
                eprintln!("Already up to date.");
            }
            SyncResult::NoRemote | SyncResult::NoGitRepo => {
                // Shouldn't happen since we already confirmed remote exists
            }
        }
    }

    if let Ok(Some(status)) = check_for_newer_version(CRATES_IO_URL, env!("CARGO_PKG_VERSION")) {
        eprintln!(
            "Note: blogtato {} is available (you have {}). Run `cargo install blogtato` to update.",
            status.latest, status.current
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_feed(url: &str) -> FeedSource {
        FeedSource {
            url: url.to_string(),
            title: String::new(),
            site_url: String::new(),
            description: String::new(),
            is_fetched: false,
            command: None,
        }
    }

    fn make_index(entries: &[(&str, &str)]) -> FeedIndex {
        FeedIndex {
            entries: entries
                .iter()
                .map(|(url, shorthand)| FeedEntry {
                    feed: make_feed(url),
                    id: shorthand.to_string(),
                    shorthand: shorthand.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_resolve_sync_sources_returns_all_feeds_without_selectors() {
        let index = make_index(&[
            ("https://example.com/a.xml", "df"),
            ("https://example.com/b.xml", "dg"),
        ]);

        let resolved = resolve_sync_sources(&index, &[]).unwrap();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].url, "https://example.com/a.xml");
        assert_eq!(resolved[1].url, "https://example.com/b.xml");
    }

    #[test]
    fn test_resolve_sync_sources_supports_multiple_shorthands() {
        let index = make_index(&[
            ("https://example.com/a.xml", "df"),
            ("https://example.com/b.xml", "dg"),
        ]);
        let selectors = vec!["@dg".to_string(), "@df".to_string()];

        let resolved = resolve_sync_sources(&index, &selectors).unwrap();

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].url, "https://example.com/b.xml");
        assert_eq!(resolved[1].url, "https://example.com/a.xml");
    }

    #[test]
    fn test_resolve_sync_sources_deduplicates_duplicate_selectors() {
        let index = make_index(&[
            ("https://example.com/a.xml", "df"),
            ("https://example.com/b.xml", "dg"),
        ]);
        let selectors = vec!["@df".to_string(), "@df".to_string()];

        let resolved = resolve_sync_sources(&index, &selectors).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].url, "https://example.com/a.xml");
    }

    #[test]
    fn test_resolve_sync_sources_rejects_missing_at_prefix() {
        let index = make_index(&[("https://example.com/a.xml", "df")]);
        let err = resolve_sync_sources(&index, &[String::from("df")]).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Invalid feed selector: df (expected @shorthand)"
        );
    }

    #[test]
    fn test_resolve_sync_sources_rejects_unknown_shorthand() {
        let index = make_index(&[("https://example.com/a.xml", "df")]);
        let err = resolve_sync_sources(&index, &[String::from("@dg")]).unwrap_err();

        assert_eq!(err.to_string(), "Unknown feed shorthand: @dg");
    }

    #[test]
    fn test_resolve_sync_sources_rejects_shorthand_prefix() {
        let index = make_index(&[("https://example.com/a.xml", "df")]);
        let err = resolve_sync_sources(&index, &[String::from("@d")]).unwrap_err();

        assert_eq!(err.to_string(), "Unknown feed shorthand: @d");
    }
}
