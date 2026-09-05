use std::collections::{HashMap, HashSet};

use crate::data::BlogData;
use crate::data::index::{FeedIndex, feed_index};
use crate::data::schema::FeedItem;
use crate::display::build_feed_labels;
use crate::shorthand::{RESERVED_COMMANDS, index_to_shorthand};

use super::{Query, ReadFilter};

pub(crate) struct PostIndex {
    pub items: Vec<(String, FeedItem)>,
    pub shorthands: HashMap<String, String>,
}

impl PostIndex {
    fn filter_by_shorthands(&mut self, shorthands: &[String]) -> anyhow::Result<()> {
        if shorthands.is_empty() {
            return Ok(());
        }
        let sh_set: HashSet<&str> = shorthands.iter().map(|s| s.as_str()).collect();
        let all_known: HashSet<&str> = self.shorthands.values().map(|s| s.as_str()).collect();
        for sh in shorthands {
            if !all_known.contains(sh.as_str()) {
                anyhow::bail!("Unknown shorthand: {sh}");
            }
        }
        self.items.retain(|(_, item)| {
            self.shorthands
                .get(&item.raw_id)
                .is_some_and(|s| sh_set.contains(s.as_str()))
        });
        Ok(())
    }

    fn filter_by_id(&mut self, id: &str) -> anyhow::Result<()> {
        let before = self.items.len();
        self.items.retain(|(item_id, _)| item_id == id);
        if self.items.is_empty() && before > 0 {
            anyhow::bail!("No post found with id: {id}");
        } else if self.items.is_empty() {
            anyhow::bail!("No posts found");
        }
        Ok(())
    }

    fn filter_by_feed(&mut self, fi: &FeedIndex, shorthand: &str) -> anyhow::Result<()> {
        let feed_id = fi
            .id_for_shorthand(shorthand)
            .ok_or_else(|| anyhow::anyhow!("Unknown feed shorthand: @{shorthand}"))?;
        self.items.retain(|(_, item)| item.feed == feed_id);
        Ok(())
    }

    fn filter_by_date(&mut self, query: &Query) {
        if let Some(ref since) = query.date_filter.since {
            self.items
                .retain(|(_, item)| item.date.is_some_and(|d| d >= since.resolved));
        }
        if let Some(ref until) = query.date_filter.until {
            self.items
                .retain(|(_, item)| item.date.is_some_and(|d| d <= until.resolved));
        }
    }

    fn filter_by_read_status(&mut self, filter: ReadFilter, store: &BlogData) {
        match filter {
            ReadFilter::Read | ReadFilter::Unread => {
                let read_ids: HashSet<&str> = store
                    .reads()
                    .iter()
                    .map(|(_, r)| r.post_id.as_str())
                    .collect();
                let keep_read = matches!(filter, ReadFilter::Read);
                self.items
                    .retain(|(_, item)| read_ids.contains(item.raw_id.as_str()) == keep_read);
            }
            ReadFilter::Any | ReadFilter::All => {}
        }
    }
}

pub(crate) fn post_index(table: &synctato::Table<FeedItem>) -> PostIndex {
    let mut items: Vec<(String, FeedItem)> = table
        .iter()
        .map(|(id, item)| (id.to_string(), item.clone()))
        .collect();
    items.sort_by(|a, b| {
        b.1.date
            .cmp(&a.1.date)
            .then_with(|| a.1.raw_id.cmp(&b.1.raw_id))
    });
    let mut idx = 0;
    let shorthands = items
        .iter()
        .map(|(_, item)| {
            loop {
                let sh = index_to_shorthand(idx);
                idx += 1;
                if !RESERVED_COMMANDS.contains(&sh.as_str()) {
                    return (item.raw_id.clone(), sh);
                }
            }
        })
        .collect();
    PostIndex { items, shorthands }
}

pub(crate) struct ResolvedPosts {
    pub items: Vec<(String, FeedItem)>,
    pub shorthands: HashMap<String, String>,
    pub feed_labels: HashMap<String, String>,
}

pub(crate) fn resolve_posts(store: &BlogData, query: &Query) -> anyhow::Result<ResolvedPosts> {
    let fi = feed_index(store.feeds());
    let feed_labels = build_feed_labels(&fi);
    let mut posts = post_index(store.posts());
    posts.filter_by_shorthands(&query.shorthands)?;
    if let Some(ref shorthand) = query.filter {
        posts.filter_by_feed(&fi, shorthand)?;
    }
    if let Some(ref id) = query.id_filter {
        posts.filter_by_id(id)?;
    }
    posts.filter_by_date(query);
    posts.filter_by_read_status(query.read_filter, store);

    Ok(ResolvedPosts {
        items: posts.items,
        shorthands: posts.shorthands,
        feed_labels,
    })
}
