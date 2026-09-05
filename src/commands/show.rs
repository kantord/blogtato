use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};

use anyhow::ensure;

use crate::data::BlogData;
use crate::data::schema::FeedItem;
use crate::display::{RenderCtx, Style, render_grouped};
use crate::query::Query;
use crate::query::resolve::resolve_posts;

pub(crate) fn cmd_show(
    store: &BlogData,
    query: &Query,
    query_text: &str,
    compact: bool,
) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");

    let read_ids: HashSet<String> = store
        .reads()
        .iter()
        .map(|(_, r)| r.post_id.clone())
        .collect();

    // Drives both ANSI styling and the pager/height decision below — kept as
    // one binding so the two never drift apart (e.g. behind a future
    // `--color` flag that shouldn't also disable paging).
    let is_tty = std::io::stdout().is_terminal();
    let terminal = terminal_size::terminal_size();
    let max_width = terminal.map(|(w, _)| w.0 as usize);
    let refs: Vec<&FeedItem> = resolved.items.iter().map(|(_, item)| item).collect();
    let ctx = RenderCtx {
        all_keys: &query.keys,
        shorthands: &resolved.shorthands,
        feed_labels: &resolved.feed_labels,
        read_ids: &read_ids,
        color: is_tty,
        shorthand_width: RenderCtx::shorthand_width_from(&refs, &resolved.shorthands),
        max_width,
        compact,
    };
    let mut output = render_grouped(&refs, &ctx);
    output.truncate(output.trim_end().len());
    output.push('\n');

    let term_height = terminal.map(|(_, h)| h.0 as usize).unwrap_or(usize::MAX);
    // Compact mode reduces line_count by removing blank separators, so it can
    // also reduce how often the pager kicks in below — this is intentional,
    // not a bug: denser output needs the pager less often.
    let line_count = output.lines().count();

    // Leave one line free so the last line of output doesn't sit flush
    // against the next shell prompt.
    if is_tty && line_count > term_height.saturating_sub(1) {
        if output_with_pager(&output).is_err() {
            print!("{output}");
        }
    } else {
        print!("{output}");
    }

    // Summary goes to stderr so it doesn't pollute piped/redirected output
    eprint!("{}", format_summary(&refs, query_text, is_tty));

    Ok(())
}

fn resolve_pager(pager_env: Option<&str>) -> (String, Vec<String>) {
    let mut parts = pager_env
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("less")
        .split_whitespace();
    let bin = parts.next().unwrap_or("less").to_string();
    let mut args: Vec<String> = parts.map(str::to_string).collect();

    let is_less = std::path::Path::new(&bin)
        .file_name()
        .and_then(|n| n.to_str())
        == Some("less");
    if is_less && !args.iter().any(|a| a == "-R") {
        args.push("-R".to_string());
    }

    (bin, args)
}

fn output_with_pager(content: &str) -> io::Result<()> {
    let (bin, args) = resolve_pager(std::env::var("PAGER").ok().as_deref());

    let mut child = std::process::Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    let result = child.stdin.as_mut().unwrap().write_all(content.as_bytes());

    // Ignore broken pipe — user quit the pager early, which is intentional
    if let Err(ref e) = result
        && e.kind() != io::ErrorKind::BrokenPipe
    {
        result?;
    }

    // Drop stdin to signal EOF
    drop(child.stdin.take());

    let _ = child.wait();
    Ok(())
}

pub(crate) fn format_summary(items: &[&FeedItem], query_text: &str, color: bool) -> String {
    let count = items.len();
    let feed_count = {
        let mut feeds: Vec<&str> = items.iter().map(|i| i.feed.as_str()).collect();
        feeds.sort_unstable();
        feeds.dedup();
        feeds.len()
    };

    let s = Style::new(color);
    format!(
        "{}{count} Post(s) from {feed_count} Feed(s) ({query_text}){}\n",
        s.dim, s.reset
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(title: &str, feed: &str, raw_id: &str) -> FeedItem {
        FeedItem {
            title: title.to_string(),
            date: None,
            feed: feed.to_string(),
            link: String::new(),
            raw_id: raw_id.to_string(),
        }
    }

    #[test]
    fn test_format_summary_multiple_posts_multiple_feeds() {
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed2", "id-b"),
            make_item("C", "feed1", "id-c"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let summary = format_summary(&refs, ".unread 90d.. /w", false);
        assert_eq!(summary, "3 Post(s) from 2 Feed(s) (.unread 90d.. /w)\n");
    }

    #[test]
    fn test_format_summary_single_post_single_feed() {
        let items = vec![make_item("A", "feed1", "id-a")];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let summary = format_summary(&refs, ".all", false);
        assert_eq!(summary, "1 Post(s) from 1 Feed(s) (.all)\n");
    }

    #[test]
    fn test_format_summary_custom_query() {
        let items = vec![
            make_item("A", "feed1", "id-a"),
            make_item("B", "feed2", "id-b"),
        ];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let summary = format_summary(&refs, "@myblog .read 2w..", false);
        assert_eq!(summary, "2 Post(s) from 2 Feed(s) (@myblog .read 2w..)\n");
    }

    #[test]
    fn test_format_summary_no_color_no_ansi() {
        let items = vec![make_item("A", "feed1", "id-a")];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let summary = format_summary(&refs, ".unread", false);
        assert!(!summary.contains("\x1b"));
    }

    #[test]
    fn test_format_summary_color_has_dim() {
        let items = vec![make_item("A", "feed1", "id-a")];
        let refs: Vec<&FeedItem> = items.iter().collect();
        let summary = format_summary(&refs, ".unread", true);
        assert!(summary.contains("\x1b[2m"));
        assert!(summary.contains("\x1b[0m"));
    }

    #[test]
    fn test_resolve_pager_defaults_to_less_with_raw_control_chars() {
        assert_eq!(
            resolve_pager(None),
            ("less".to_string(), vec!["-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_empty_env_falls_back_to_less() {
        assert_eq!(
            resolve_pager(Some("   ")),
            ("less".to_string(), vec!["-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_bare_less_gets_raw_control_chars() {
        assert_eq!(
            resolve_pager(Some("less")),
            ("less".to_string(), vec!["-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_less_with_flags_still_gets_raw_control_chars() {
        assert_eq!(
            resolve_pager(Some("less -F")),
            ("less".to_string(), vec!["-F".to_string(), "-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_less_with_existing_raw_flag_not_duplicated() {
        assert_eq!(
            resolve_pager(Some("less -R")),
            ("less".to_string(), vec!["-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_full_path_to_less_still_recognized() {
        assert_eq!(
            resolve_pager(Some("/usr/bin/less")),
            ("/usr/bin/less".to_string(), vec!["-R".to_string()])
        );
    }

    #[test]
    fn test_resolve_pager_splits_multiple_args_for_non_less_pager() {
        assert_eq!(
            resolve_pager(Some("bat --paging=always --style=plain")),
            (
                "bat".to_string(),
                vec!["--paging=always".to_string(), "--style=plain".to_string()]
            )
        );
    }

    #[test]
    fn test_resolve_pager_non_less_gets_no_extra_flags() {
        assert_eq!(resolve_pager(Some("most")), ("most".to_string(), vec![]));
    }
}
