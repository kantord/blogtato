pub mod atom;
pub(crate) mod discover;
pub(crate) mod pull;
pub mod rss;

use crate::data::schema::FeedItem;

#[derive(Debug, Clone, PartialEq)]
pub struct FeedMeta {
    pub title: String,
    pub site_url: String,
    pub description: String,
}

fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn sanitize_parsed(meta: FeedMeta, items: Vec<FeedItem>) -> (FeedMeta, Vec<FeedItem>) {
    let meta = FeedMeta {
        title: sanitize(&meta.title),
        site_url: sanitize(&meta.site_url),
        description: sanitize(&meta.description),
    };
    let items = items
        .into_iter()
        .map(|item| FeedItem {
            title: sanitize(&item.title),
            link: sanitize(&item.link),
            raw_id: sanitize(&item.raw_id),
            ..item
        })
        .collect();
    (meta, items)
}

pub(crate) fn parse(bytes: &[u8]) -> anyhow::Result<(FeedMeta, Vec<FeedItem>)> {
    let (meta, items) = rss::parse(bytes).or_else(|_| atom::parse(bytes))?;
    Ok(sanitize_parsed(meta, items))
}

pub fn fetch(client: &ureq::Agent, url: &str) -> anyhow::Result<(FeedMeta, Vec<FeedItem>)> {
    let bytes = client.get(url).call()?.body_mut().read_to_vec()?;
    parse(&bytes[..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rstest::rstest;

    const MALICIOUS: &str = "Evil \x1b[31mRed\x1b[0m Text";

    fn rss_xml(field: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <rss version="2.0">
              <channel>
                <title>{title}</title>
                <link>{site_url}</link>
                <description>{description}</description>
                <item>
                  <title>{item_title}</title>
                  <guid>{item_raw_id}</guid>
                  <link>{item_link}</link>
                </item>
              </channel>
            </rss>"#,
            title = if field == "feed_title" {
                MALICIOUS
            } else {
                "Clean"
            },
            site_url = if field == "feed_site_url" {
                MALICIOUS
            } else {
                "https://example.com"
            },
            description = if field == "feed_description" {
                MALICIOUS
            } else {
                "Clean"
            },
            item_title = if field == "item_title" {
                MALICIOUS
            } else {
                "Clean"
            },
            item_raw_id = if field == "item_raw_id" {
                MALICIOUS
            } else {
                "urn:test:1"
            },
            item_link = if field == "item_link" {
                MALICIOUS
            } else {
                "https://example.com/post"
            },
        )
    }

    #[rstest]
    #[case::feed_title("feed_title")]
    #[case::feed_description("feed_description")]
    #[case::feed_site_url("feed_site_url")]
    #[case::item_title("item_title")]
    #[case::item_link("item_link")]
    #[case::item_raw_id("item_raw_id")]
    fn test_control_characters_are_stripped(#[case] field: &str) {
        let xml = rss_xml(field);
        let (meta, items) = parse(xml.as_bytes()).unwrap();
        let actual = match field {
            "feed_title" => &meta.title,
            "feed_description" => &meta.description,
            "feed_site_url" => &meta.site_url,
            "item_title" => &items[0].title,
            "item_link" => &items[0].link,
            "item_raw_id" => &items[0].raw_id,
            _ => unreachable!(),
        };
        assert!(
            !actual.contains('\x1b'),
            "{field} should not contain escape characters, got: {actual:?}"
        );
    }

    #[test]
    fn test_serde_roundtrip_with_date() {
        let item = FeedItem {
            title: "Test Post".to_string(),
            date: Some(
                NaiveDate::from_ymd_opt(2024, 1, 15)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap()
                    .and_utc(),
            ),
            feed: "abc123".to_string(),
            link: String::new(),
            raw_id: String::new(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }

    #[test]
    fn test_serde_roundtrip_without_date() {
        let item = FeedItem {
            title: "No Date Post".to_string(),
            date: None,
            feed: "def456".to_string(),
            link: String::new(),
            raw_id: String::new(),
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: FeedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, deserialized);
    }
}
