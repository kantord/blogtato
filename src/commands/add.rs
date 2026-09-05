use anyhow::bail;

use crate::data::Transaction;
use crate::data::schema::FeedSource;
use crate::utils::progress::spinner;

/// Sanity cap on feed candidates to validate during HTML discovery.
const MAX_FEED_CANDIDATES: usize = 20;

pub(crate) fn resolve_feed_url(url: &str) -> anyhow::Result<String> {
    let client = crate::utils::http::http_client();

    let sp = spinner(&format!("Fetching {url}..."));
    let bytes = client.get(url).call()?.body_mut().read_to_vec()?;

    // Try parsing as RSS/Atom — if it works, the URL is already a feed
    if is_feed_content(&bytes) {
        sp.finish_and_clear();
        return Ok(url.to_string());
    }

    // Not a feed — try HTML feed discovery
    sp.set_message(format!("Looking for feeds on {url}..."));
    let html = String::from_utf8_lossy(&bytes);
    let base_url = url::Url::parse(url)?;
    let candidates = crate::feed::discover::discover_feed_urls(&html, &base_url);

    // Validate candidates by fetching and parsing each one
    let feeds: Vec<_> = candidates
        .into_iter()
        .take(MAX_FEED_CANDIDATES)
        .filter(|u| {
            sp.set_message(format!("Checking {u}..."));
            is_valid_feed(&client, u)
        })
        .collect();

    sp.finish_and_clear();

    match feeds.len() {
        0 => bail!("no feeds found at {url}"),
        1 => Ok(feeds.into_iter().next().unwrap()),
        _ => {
            eprintln!("Multiple feeds found at {url}:");
            for feed_url in &feeds {
                eprintln!("  {feed_url}");
            }
            bail!(
                "multiple feeds found; run `blog feed add <feed-url>` with a specific URL from the list above"
            );
        }
    }
}

fn is_feed_content(bytes: &[u8]) -> bool {
    crate::feed::rss::parse(bytes).is_ok() || crate::feed::atom::parse(bytes).is_ok()
}

fn is_valid_feed(client: &ureq::Agent, url: &str) -> bool {
    let Ok(mut resp) = client.get(url).call() else {
        return false;
    };
    let Ok(bytes) = resp.body_mut().read_to_vec() else {
        return false;
    };
    is_feed_content(&bytes)
}

fn normalize_feed_url(url: &str) -> String {
    url_normalize::normalize_url(url, &url_normalize::Options::default())
        .unwrap_or_else(|_| url.to_string())
}

pub(crate) fn cmd_add(tx: &mut Transaction, url: &str) -> anyhow::Result<()> {
    let url = normalize_feed_url(url);
    tx.feeds.upsert(FeedSource {
        url,
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
        is_fetched: false,
    });
    Ok(())
}
