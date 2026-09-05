use std::fmt::Write;

use itertools::Itertools;

use crate::data::schema::FeedItem;
use crate::query::GroupKey;

use super::item::format_item;
use super::{RenderCtx, Style};

pub(crate) fn render_grouped(items: &[&FeedItem], ctx: &RenderCtx) -> String {
    fn recurse(out: &mut String, items: &[&FeedItem], remaining: &[GroupKey], ctx: &RenderCtx) {
        let depth = ctx.all_keys.len() - remaining.len();
        let indent = "  ".repeat(depth);

        if remaining.is_empty() {
            let indent_width = depth * 2;
            let content_width = ctx.max_width.map(|w| w.saturating_sub(indent_width));
            for item in items {
                writeln!(out, "{indent}{}", format_item(item, content_width, ctx)).unwrap();
            }
            return;
        }

        let key = remaining[0];
        let rest = &remaining[1..];

        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| key.compare(a, b, ctx.feed_labels));

        let s = Style::new(ctx.color);

        let (prefix, suffix) = if depth == 0 {
            ("=== ", " ===")
        } else {
            ("--- ", " ---")
        };

        for (group_val, group) in &sorted
            .iter()
            .chunk_by(|item| key.extract(item, ctx.feed_labels))
        {
            let group_items: Vec<&FeedItem> = group.copied().collect();
            writeln!(
                out,
                "{indent}{}{prefix}{group_val}{suffix}{}",
                s.bold, s.reset
            )
            .unwrap();
            if depth == 0 && !ctx.compact {
                writeln!(out).unwrap();
            }
            recurse(out, &group_items, rest, ctx);
            if !ctx.compact {
                if depth == 0 {
                    writeln!(out).unwrap();
                    writeln!(out).unwrap();
                } else {
                    writeln!(out).unwrap();
                }
            }
        }
    }

    let mut out = String::new();
    recurse(&mut out, items, ctx.all_keys, ctx);
    out
}
