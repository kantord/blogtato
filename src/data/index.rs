use crate::data::schema::FeedSource;
use crate::shorthand::compute_shorthands;

pub(crate) struct FeedEntry {
    pub feed: FeedSource,
    pub id: String,
    pub shorthand: String,
}

pub(crate) struct FeedIndex {
    pub entries: Vec<FeedEntry>,
}

impl FeedIndex {
    fn find_by_shorthand(&self, shorthand: &str) -> Option<&FeedEntry> {
        // Accept any unambiguous match: either the input starts with a known
        // shorthand, or a known shorthand starts with the input.
        let mut iter = self
            .entries
            .iter()
            .filter(|e| e.shorthand.starts_with(shorthand) || shorthand.starts_with(&e.shorthand));
        let first = iter.next()?;
        if iter.next().is_some() {
            None // ambiguous
        } else {
            Some(first)
        }
    }

    pub(crate) fn id_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.find_by_shorthand(shorthand).map(|e| e.id.as_str())
    }

    pub(crate) fn url_for_shorthand(&self, shorthand: &str) -> Option<&str> {
        self.find_by_shorthand(shorthand)
            .map(|e| e.feed.url.as_str())
    }
}

pub(crate) fn feed_index(table: &synctato::Table<FeedSource>) -> FeedIndex {
    let mut pairs: Vec<(String, FeedSource)> = table
        .iter()
        .map(|(id, feed)| (id.to_string(), feed.clone()))
        .collect();
    pairs.sort_by(|(_, a), (_, b)| a.url.cmp(&b.url));
    let ids: Vec<String> = pairs.iter().map(|(id, _)| id.clone()).collect();
    let shorthands = compute_shorthands(&ids);
    let entries = pairs
        .into_iter()
        .zip(shorthands)
        .map(|((id, feed), shorthand)| FeedEntry {
            feed,
            id,
            shorthand,
        })
        .collect();
    FeedIndex { entries }
}

pub(crate) fn resolve_shorthand(
    feeds_table: &synctato::Table<FeedSource>,
    shorthand: &str,
) -> Option<String> {
    feed_index(feeds_table)
        .url_for_shorthand(shorthand)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shorthand::compute_shorthands;

    fn make_feed(url: &str) -> FeedSource {
        FeedSource {
            url: url.to_string(),
            title: String::new(),
            site_url: String::new(),
            description: String::new(),
            is_fetched: false,
        }
    }

    fn make_index_from_ids(ids: &[&str], urls: &[&str]) -> FeedIndex {
        let id_strings: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        let shorthands = compute_shorthands(&id_strings);
        FeedIndex {
            entries: ids
                .iter()
                .zip(shorthands)
                .zip(urls)
                .map(|((id, shorthand), url)| FeedEntry {
                    id: id.to_string(),
                    shorthand,
                    feed: make_feed(url),
                })
                .collect(),
        }
    }

    #[test]
    fn test_exact_shorthand_matches() {
        let index =
            make_index_from_ids(&["00", "ff"], &["https://a.com/feed", "https://b.com/feed"]);
        let sh0 = index.entries[0].shorthand.clone();
        let sh1 = index.entries[1].shorthand.clone();
        assert_eq!(index.id_for_shorthand(&sh0), Some("00"));
        assert_eq!(index.id_for_shorthand(&sh1), Some("ff"));
    }

    #[test]
    fn test_old_longer_shorthand_resolves_after_feed_deletion() {
        // Three feeds with similar IDs require longer shorthands
        let ids_before = &["aa", "ab", "ff"];
        let urls = &[
            "https://a.com/feed",
            "https://b.com/feed",
            "https://c.com/feed",
        ];
        let index_before = make_index_from_ids(ids_before, urls);
        let old_shorthand_aa = index_before.entries[0].shorthand.clone();
        let old_shorthand_ab = index_before.entries[1].shorthand.clone();

        // Shorthands for "aa" and "ab" should be longer than 1 char
        // because they share a prefix in home-row encoding
        assert!(
            old_shorthand_aa.len() > 1 || old_shorthand_ab.len() > 1,
            "similar IDs should produce longer shorthands"
        );

        // Now delete feed "ab" — shorthands recompute and become shorter
        let index_after =
            make_index_from_ids(&["aa", "ff"], &["https://a.com/feed", "https://c.com/feed"]);
        let new_shorthand_aa = index_after.entries[0].shorthand.clone();

        // The new shorthand should be shorter (or equal)
        assert!(new_shorthand_aa.len() <= old_shorthand_aa.len());

        // The OLD longer shorthand should still resolve to the same feed
        assert_eq!(
            index_after.id_for_shorthand(&old_shorthand_aa),
            Some("aa"),
            "old shorthand {:?} should still resolve after feed deletion (current shorthand is {:?})",
            old_shorthand_aa,
            new_shorthand_aa,
        );
    }

    #[test]
    fn test_ambiguous_prefix_returns_none() {
        // With "aa" and "ab", a short prefix that matches both should fail
        let index =
            make_index_from_ids(&["aa", "ab"], &["https://a.com/feed", "https://b.com/feed"]);
        // The 1-char prefix of both home-row encodings should be the same
        let prefix = &index.entries[0].shorthand[..1];
        if prefix == &index.entries[1].shorthand[..1] {
            assert_eq!(
                index.id_for_shorthand(prefix),
                None,
                "ambiguous prefix should not match"
            );
        }
    }

    #[test]
    fn test_nonexistent_shorthand_returns_none() {
        let index = make_index_from_ids(&["00"], &["https://a.com/feed"]);
        assert_eq!(index.id_for_shorthand("zzzzz"), None);
    }
}
