use crate::data::Transaction;
use crate::data::schema::FeedSource;
use crate::feed::pull::apply_feed;

pub(crate) fn cmd_ingest(tx: &mut Transaction, name: &str, bytes: &[u8]) -> anyhow::Result<String> {
    anyhow::ensure!(!name.is_empty(), "feed name cannot be empty");
    anyhow::ensure!(
        !name.starts_with("stdin:"),
        "feed name should not include the stdin: prefix"
    );
    anyhow::ensure!(
        name.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "feed name may only contain letters, numbers, dots, dashes, and underscores"
    );

    let url = format!("stdin:{name}");
    let (meta, items) = crate::feed::parse(bytes)?;

    let source = tx.feeds.get(&url).cloned().unwrap_or_else(|| FeedSource {
        url: url.clone(),
        title: String::new(),
        site_url: String::new(),
        description: String::new(),
        is_fetched: false,
    });

    apply_feed(tx, source, meta, items);
    Ok(url)
}
