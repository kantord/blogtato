use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};

use anyhow::ensure;

use crate::data::BlogData;
use crate::data::schema::FeedItem;
use crate::display::{RenderCtx, Style, render_grouped};
use crate::query::Query;
use crate::query::resolve::resolve_posts;

pub(crate) fn cmd_show(store: &BlogData, query: &Query, query_text: &str) -> anyhow::Result<()> {
    let resolved = resolve_posts(store, query)?;
    ensure!(!resolved.items.is_empty(), "No matching posts");

    let read_ids: HashSet<String> = store
        .reads()
        .iter()
        .map(|(_, r)| r.post_id.clone())
        .collect();

    let color = std::io::stdout().is_terminal();
    let terminal = terminal_size::terminal_size();
    let max_width = terminal.map(|(w, _)| w.0 as usize);
    let refs: Vec<&FeedItem> = resolved.items.iter().map(|(_, item)| item).collect();
    let ctx = RenderCtx {
        all_keys: &query.keys,
        shorthands: &resolved.shorthands,
        feed_labels: &resolved.feed_labels,
        read_ids: &read_ids,
        color,
        shorthand_width: RenderCtx::shorthand_width_from(&refs, &resolved.shorthands),
        max_width,
    };
    let mut output = render_grouped(&refs, &ctx);
    // Trim trailing blank lines from grouped output
    let trimmed_len = output.trim_end().len();
    output.truncate(trimmed_len);
    output.push('\n');

    let term_height = terminal.map(|(_, h)| h.0 as usize).unwrap_or(usize::MAX);
    let line_count = output.lines().count();

    if color && line_count > term_height {
        if output_with_pager(&output).is_err() {
            print!("{output}");
        }
    } else {
        print!("{output}");
    }

    // Summary goes to stderr so it doesn't pollute piped/redirected output
    eprint!("{}", format_summary(&refs, query_text, color));

    Ok(())
}

fn output_with_pager(content: &str) -> io::Result<()> {
    let pager_env = std::env::var("PAGER").ok();
    let (bin, args) = match &pager_env {
        Some(val) => {
            let val = val.trim();
            match val.split_once(char::is_whitespace) {
                Some((b, a)) => (b.to_string(), vec![a.to_string()]),
                None => {
                    let mut args = Vec::new();
                    if val == "less" {
                        args.push("-R".to_string());
                    }
                    (val.to_string(), args)
                }
            }
        }
        None => ("less".to_string(), vec!["-R".to_string()]),
    };

    let mut child = std::process::Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    let result = child.stdin.as_mut().unwrap().write_all(content.as_bytes());

    // Ignore broken pipe — user quit the pager early, which is intentional
    if let Err(ref e) = result {
        if e.kind() != io::ErrorKind::BrokenPipe {
            result?;
        }
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
}
