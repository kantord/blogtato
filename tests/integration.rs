use std::fs;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use httpmock::prelude::*;
use rstest::rstest;
use tempfile::TempDir;

/// Return an RFC 2822 date string `days_ago` days before now.
fn recent_rss_date(days_ago: i64) -> String {
    let dt = chrono::Utc::now() - chrono::Duration::days(days_ago);
    dt.format("%a, %d %b %Y %H:%M:%S +0000").to_string()
}

trait AssertExt {
    fn stderr_str(&self) -> String;
    fn stdout_str(&self) -> String;
}

impl AssertExt for Assert {
    fn stderr_str(&self) -> String {
        String::from_utf8(self.get_output().stderr.clone()).unwrap()
    }
    fn stdout_str(&self) -> String {
        String::from_utf8(self.get_output().stdout.clone()).unwrap()
    }
}

fn read_table(dir: &Path) -> Vec<serde_json::Value> {
    let mut items = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                && fname.starts_with("items_")
                && fname.ends_with(".jsonl")
            {
                let file = fs::File::open(&path).unwrap();
                for line in std::io::BufReader::new(file).lines() {
                    let line = line.unwrap();
                    if !line.trim().is_empty() {
                        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
                        if value.get("deleted_at").is_none() {
                            items.push(value);
                        }
                    }
                }
            }
        }
    }
    items
}

struct TestContext {
    dir: TempDir,
    server: MockServer,
}

impl TestContext {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
            server: MockServer::start(),
        }
    }

    fn write_feeds(&self, urls: &[&str]) {
        let feeds_dir = self.dir.path().join("feeds");
        if feeds_dir.exists() {
            fs::remove_dir_all(&feeds_dir).unwrap();
        }
        for url in urls {
            insert_feed(self.dir.path(), url);
        }
    }

    fn read_posts(&self) -> Vec<serde_json::Value> {
        read_table(&self.dir.path().join("posts"))
    }

    fn read_feeds(&self) -> Vec<serde_json::Value> {
        read_table(&self.dir.path().join("feeds"))
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        blog_cmd()
            .args(args)
            .env("RSS_STORE", self.dir.path())
            .assert()
    }

    fn run_with_stdin(&self, args: &[&str], stdin: &str) -> assert_cmd::assert::Assert {
        #[allow(deprecated)]
        Command::cargo_bin("blog")
            .unwrap()
            .args(args)
            .env("RSS_STORE", self.dir.path())
            .write_stdin(stdin)
            .assert()
    }

    fn mock_rss_feed(&self, path: &str, xml: &str) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/rss+xml")
                .body(xml);
        });
    }

    fn mock_rss_feed_bytes(&self, path: &str, body: &[u8]) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/rss+xml")
                .body(body);
        });
    }

    fn mock_atom_feed(&self, path: &str, xml: &str) {
        self.server.mock(|when, then| {
            when.method(GET).path(path);
            then.status(200)
                .header("Content-Type", "application/atom+xml")
                .body(xml);
        });
    }
}

fn rss_xml_with_links(title: &str, items: &[(&str, &str, &str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .map(|(item_title, date, guid, link)| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>{}</guid><link>{}</link></item>",
                item_title, date, guid, link
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{}</title>
    {}
  </channel>
</rss>"#,
        title, items_xml
    )
}

fn rss_xml_with_guids(title: &str, items: &[(&str, &str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .map(|(item_title, date, guid)| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>{}</guid></item>",
                item_title, date, guid
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{}</title>
    {}
  </channel>
</rss>"#,
        title, items_xml
    )
}

fn rss_xml(title: &str, items: &[(&str, &str)]) -> String {
    let items_xml: String = items
        .iter()
        .enumerate()
        .map(|(i, (item_title, date))| {
            format!(
                "<item><title>{}</title><pubDate>{}</pubDate><guid>urn:item:{}</guid></item>",
                item_title, date, i
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{}</title>
    {}
  </channel>
</rss>"#,
        title, items_xml
    )
}

fn atom_xml(title: &str, feed_id: &str, entries: &[(&str, &str, &str)]) -> String {
    let entries_xml: String = entries
        .iter()
        .map(|(entry_title, id, date)| {
            format!(
                "<entry><title>{}</title><id>{}</id><updated>{}</updated><published>{}</published></entry>",
                entry_title, id, date, date
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>{}</title>
  <id>{}</id>
  <updated>2024-01-01T00:00:00Z</updated>
  {}
</feed>"#,
        title, feed_id, entries_xml
    )
}

fn feed_shorthand_for_url(ctx: &TestContext, url: &str) -> String {
    let output = ctx.run(&["feed", "ls"]).success();
    output
        .stdout_str()
        .lines()
        .find(|line| line.contains(url))
        .map(|line| line.chars().take_while(|c| *c != ' ').collect())
        .expect("should find url in feed ls output")
}

#[test]
fn test_sync_creates_posts_file() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Test Blog",
        &[
            ("First Post", "Mon, 01 Jan 2024 00:00:00 +0000"),
            ("Second Post", "Tue, 02 Jan 2024 00:00:00 +0000"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);
    let titles: Vec<&str> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"First Post"));
    assert!(titles.contains(&"Second Post"));
    // feed field should contain the feed's table ID, same for all posts from this feed
    let feed_ids: Vec<&str> = posts.iter().map(|p| p["feed"].as_str().unwrap()).collect();
    assert!(feed_ids.iter().all(|f| !f.is_empty()));
    assert!(feed_ids.iter().all(|f| f == &feed_ids[0]));
}

#[test]
fn test_sync_multiple_feeds() {
    let ctx = TestContext::new();

    let rss = rss_xml(
        "RSS Blog",
        &[("RSS Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/rss.xml", &rss);

    let atom = atom_xml(
        "Atom Blog",
        "urn:atom-blog",
        &[("Atom Post", "urn:atom:1", "2024-01-02T00:00:00Z")],
    );
    ctx.mock_atom_feed("/atom.xml", &atom);

    let rss_url = ctx.server.url("/rss.xml");
    let atom_url = ctx.server.url("/atom.xml");
    ctx.write_feeds(&[&rss_url, &atom_url]);

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    let titles: Vec<&str> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"RSS Post"));
    assert!(titles.contains(&"Atom Post"));
}

#[test]
fn test_sync_selected_feed_by_shorthand() {
    let ctx = TestContext::new();

    let alpha = rss_xml_with_guids(
        "Alpha Blog",
        &[(
            "Alpha Post",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-alpha",
        )],
    );
    ctx.mock_rss_feed("/alpha.xml", &alpha);

    let beta = rss_xml_with_guids(
        "Beta Blog",
        &[("Beta Post", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-beta")],
    );
    ctx.mock_rss_feed("/beta.xml", &beta);

    let alpha_url = ctx.server.url("/alpha.xml");
    let beta_url = ctx.server.url("/beta.xml");
    ctx.write_feeds(&[&alpha_url, &beta_url]);

    let alpha_shorthand = feed_shorthand_for_url(&ctx, &alpha_url);
    ctx.run(&["sync", "--feed", &alpha_shorthand]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Alpha Post");
}

#[test]
fn test_sync_multiple_selected_feeds_by_shorthand() {
    let ctx = TestContext::new();

    let alpha = rss_xml_with_guids(
        "Alpha Blog",
        &[(
            "Alpha Post",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-alpha",
        )],
    );
    ctx.mock_rss_feed("/alpha.xml", &alpha);

    let beta = rss_xml_with_guids(
        "Beta Blog",
        &[("Beta Post", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-beta")],
    );
    ctx.mock_rss_feed("/beta.xml", &beta);

    let gamma = rss_xml_with_guids(
        "Gamma Blog",
        &[(
            "Gamma Post",
            "Wed, 03 Jan 2024 00:00:00 +0000",
            "guid-gamma",
        )],
    );
    ctx.mock_rss_feed("/gamma.xml", &gamma);

    let alpha_url = ctx.server.url("/alpha.xml");
    let beta_url = ctx.server.url("/beta.xml");
    let gamma_url = ctx.server.url("/gamma.xml");
    ctx.write_feeds(&[&alpha_url, &beta_url, &gamma_url]);

    let alpha_shorthand = feed_shorthand_for_url(&ctx, &alpha_url);
    let gamma_shorthand = feed_shorthand_for_url(&ctx, &gamma_url);
    ctx.run(&[
        "sync",
        "--feed",
        &alpha_shorthand,
        "--feed",
        &gamma_shorthand,
    ])
    .success();

    let posts = ctx.read_posts();
    let titles: Vec<&str> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert_eq!(posts.len(), 2);
    assert!(titles.contains(&"Alpha Post"));
    assert!(titles.contains(&"Gamma Post"));
    assert!(!titles.contains(&"Beta Post"));
}

#[test]
fn test_sync_selected_feed_requires_at_prefix() {
    let ctx = TestContext::new();
    insert_feed(ctx.dir.path(), "https://example.com/feed.xml");

    let output = ctx.run(&["sync", "--feed", "df"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Invalid feed selector: df (expected @shorthand)"),
        "expected invalid selector error, got: {}",
        stderr
    );
}

#[test]
fn test_sync_selected_feed_rejects_unknown_shorthand() {
    let ctx = TestContext::new();
    insert_feed(ctx.dir.path(), "https://example.com/feed.xml");

    let output = ctx.run(&["sync", "--feed", "@dg"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Unknown feed shorthand: @dg"),
        "expected unknown shorthand error, got: {}",
        stderr
    );
}

#[test]
fn test_show_displays_posts() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Hello World","date":"2024-01-15T00:00:00Z","feed":"Alice"}
{"id":"2","title":"Second Post","date":"2024-01-14T00:00:00Z","feed":"Bob"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "2020-01-01.."]).success();
    let stdout = output.stdout_str();

    assert!(stdout.contains("Hello World"));
    assert!(stdout.contains("Second Post"));
    assert!(stdout.contains("Alice"));
    assert!(stdout.contains("Bob"));
}

#[test]
fn test_show_rejects_too_many_grouping_args() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "/d", "/f", "/d"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Too many grouping arguments"),
        "expected error about too many grouping args, got: {}",
        stderr,
    );
}

#[test]
fn test_show_rejects_unknown_argument() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "/x"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Failed to parse argument"),
        "expected error about failed to parse argument, got: {}",
        stderr,
    );
}

#[test]
fn test_show_with_grouping() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"Alice"}
{"id":"2","title":"Post B","date":"2024-01-15T00:00:00Z","feed":"Bob"}
{"id":"3","title":"Post C","date":"2024-01-14T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "/d"]).success();
    let stdout = output.stdout_str();

    assert!(stdout.contains("=== 2024-01-15 ==="));
    assert!(stdout.contains("=== 2024-01-14 ==="));
    assert!(stdout.contains("Post A"));
    assert!(stdout.contains("Post B"));
    assert!(stdout.contains("Post C"));
}

#[test]
fn test_show_default_no_subcommand() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Default Show","date":"2024-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["2020-01-01.."]).success();
    let stdout = output.stdout_str();

    assert!(stdout.contains("Default Show"));
    assert!(stdout.contains("Alice"));
}

#[test]
fn test_sync_then_show() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Roundtrip Blog",
        &[("Roundtrip Post", "Wed, 03 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show", "2020-01-01.."]).success();
    let stdout = output.stdout_str();

    assert!(stdout.contains("Roundtrip Post"));
    assert!(stdout.contains("Roundtrip Blog"));
}

#[test]
fn test_serde_roundtrip() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Serde Blog",
        &[
            ("Post One", "Mon, 01 Jan 2024 12:00:00 +0000"),
            ("Post Two", "Tue, 02 Jan 2024 12:00:00 +0000"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    // Write back and re-read to verify roundtrip
    let mut out = String::new();
    for post in &posts {
        out.push_str(&serde_json::to_string(post).unwrap());
        out.push('\n');
    }
    // Remove existing shard files and write all to a single shard
    if let Ok(entries) = fs::read_dir(ctx.dir.path().join("posts")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(fname) = path.file_name().and_then(|f| f.to_str())
                && fname.starts_with("items_")
                && fname.ends_with(".jsonl")
            {
                fs::remove_file(&path).unwrap();
            }
        }
    }
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), &out).unwrap();

    let posts2 = ctx.read_posts();
    assert_eq!(posts, posts2);
}

#[test]
fn test_sync_twice_no_duplicates() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Blog",
        &[
            ("Post A", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-a"),
            ("Post B", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-b"),
        ],
    );
    ctx.mock_rss_feed("/feed.xml", &xml1);

    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();
    let posts1 = ctx.read_posts();
    assert_eq!(posts1.len(), 2);

    // Second pull with overlapping + new item
    let xml2 = rss_xml_with_guids(
        "Blog",
        &[
            (
                "Post B Updated",
                "Tue, 02 Jan 2024 00:00:00 +0000",
                "guid-b",
            ),
            ("Post C", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-c"),
        ],
    );
    ctx.mock_rss_feed("/feed2.xml", &xml2);

    let url2 = ctx.server.url("/feed2.xml");
    ctx.write_feeds(&[&url2]);

    ctx.run(&["sync"]).success();
    let posts2 = ctx.read_posts();

    // Should have 3 items: A (from first pull, preserved), B (updated), C (new)
    assert_eq!(posts2.len(), 3);

    let titles: Vec<&str> = posts2
        .iter()
        .map(|p| p["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Post A"));
    assert!(titles.contains(&"Post B Updated"));
    assert!(titles.contains(&"Post C"));
    // Original "Post B" should be overwritten
    assert!(!titles.contains(&"Post B"));
}

#[test]
fn test_add_creates_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml("Test Feed", &[]);
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.run(&["feed", "add", &url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), url);
}

#[test]
fn test_add_then_sync() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Added Blog",
        &[("Added Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/added.xml", &xml);

    let url = ctx.server.url("/added.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Added Post");
}

fn ingest_rss(
    title: &str,
    site_url: &str,
    description: &str,
    item_title: &str,
    guid: &str,
    link: &str,
) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>{title}</title>
    <link>{site_url}</link>
    <description>{description}</description>
    <item>
      <title>{item_title}</title>
      <pubDate>{date}</pubDate>
      <guid>{guid}</guid>
      <link>{link}</link>
    </item>
  </channel>
</rss>"#,
        date = recent_rss_date(1)
    )
}

#[test]
fn test_ingest_creates_feed_and_posts() {
    let ctx = TestContext::new();
    let xml = ingest_rss(
        "HN updates",
        "https://hnrss.org/",
        "Generated by local script",
        "Ingested Post",
        "ingested-post-1",
        "https://example.com/ingested-post",
    );

    ctx.run_with_stdin(&["feed", "ingest", "hn-updates"], &xml)
        .success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Ingested Post");
    assert_eq!(
        posts[0]["link"].as_str().unwrap(),
        "https://example.com/ingested-post"
    );

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), "stdin:hn-updates");
    assert_eq!(feeds[0]["title"].as_str().unwrap(), "HN updates");
    assert_eq!(feeds[0]["site_url"].as_str().unwrap(), "https://hnrss.org/");
    assert_eq!(
        feeds[0]["description"].as_str().unwrap(),
        "Generated by local script"
    );
    assert!(feeds[0]["is_fetched"].as_bool().unwrap());
}

#[test]
fn test_ingest_twice_updates_same_feed_without_duplicating() {
    let ctx = TestContext::new();
    let first = ingest_rss(
        "TG channel",
        "https://t.me/s/channel_name",
        "Generated with limit 20",
        "Telegram Post 1",
        "tg-1",
        "https://t.me/s/channel_name/1",
    );
    ctx.run_with_stdin(&["feed", "ingest", "tg-channel"], &first)
        .success();

    let second = ingest_rss(
        "TG channel",
        "https://t.me/s/channel_name",
        "Generated with limit 20",
        "Telegram Post 2",
        "tg-2",
        "https://t.me/s/channel_name/2",
    );
    ctx.run_with_stdin(&["feed", "ingest", "tg-channel"], &second)
        .success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), "stdin:tg-channel");

    let posts = ctx.read_posts();
    let titles: Vec<_> = posts.iter().map(|p| p["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Telegram Post 1"));
    assert!(titles.contains(&"Telegram Post 2"));
}

#[test]
fn test_ingest_then_sync_does_not_touch_stdin_feed() {
    let ctx = TestContext::new();
    let xml = ingest_rss(
        "HN updates",
        "https://hnrss.org/",
        "Generated by local script",
        "Ingested Post",
        "ingested-post-1",
        "https://example.com/ingested-post",
    );
    ctx.run_with_stdin(&["feed", "ingest", "hn-updates"], &xml)
        .success();

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), "stdin:hn-updates");
}

#[test]
fn test_sync_continues_after_feed_failure() {
    let ctx = TestContext::new();

    // One feed returns a 500 error
    ctx.server.mock(|when, then| {
        when.method(GET).path("/broken.xml");
        then.status(500).body("Internal Server Error");
    });

    // The other feed works fine
    let xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &xml);

    let broken_url = ctx.server.url("/broken.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&broken_url, &good_url]);

    let output = ctx.run(&["sync"]).success();
    let stderr = output.stderr_str();

    // Error message should mention the HTTP status, not a confusing XML parse error
    assert!(
        stderr.contains("500"),
        "error should mention HTTP 500 status, got: {}",
        stderr
    );

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Good Post");
}

#[test]
fn test_sync_reports_http_404_clearly() {
    let ctx = TestContext::new();

    ctx.server.mock(|when, then| {
        when.method(GET).path("/gone.xml");
        then.status(404).body("Not Found");
    });

    let xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &xml);

    let gone_url = ctx.server.url("/gone.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&gone_url, &good_url]);

    let output = ctx.run(&["sync"]).success();
    let stderr = output.stderr_str();

    assert!(
        stderr.contains("404"),
        "error should mention HTTP 404 status, got: {}",
        stderr
    );

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Good Post");
}

#[test]
fn test_remove_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Blog to Remove",
        &[("Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/removable.xml", &xml);

    let url = ctx.server.url("/removable.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show", "2020-01-01.."]).success();
    let stdout = output.stdout_str();
    assert!(stdout.contains("Blog to Remove"));

    ctx.run(&["feed", "rm", &url]).success();

    // Pull should no longer fetch the removed feed
    ctx.run(&["sync"]).success();

    // Feed and its posts should be gone — show should report no posts
    let output = ctx.run(&["show", "2020-01-01.."]).failure();
    let stderr = output.stderr_str();
    assert!(stderr.contains("No matching posts"));
}

#[test]
fn test_remove_feed_deletes_its_posts() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Keep Blog",
        &[("Keep Post", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-keep")],
    );
    ctx.mock_rss_feed("/keep.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Remove Blog",
        &[(
            "Remove Post",
            "Tue, 02 Jan 2024 00:00:00 +0000",
            "guid-remove",
        )],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 2);

    ctx.run(&["feed", "rm", &remove_url]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Keep Post");
}

#[test]
fn test_remove_feed_cleans_up_read_marks() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let xml = rss_xml_with_links(
        "Doomed Blog",
        &[(
            "Doomed Post",
            &date_a,
            "guid-doomed",
            "https://example.com/doomed",
        )],
    );
    ctx.mock_rss_feed("/doomed.xml", &xml);
    let url = ctx.server.url("/doomed.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Mark the post as read by opening it
    blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert()
        .success();

    // Verify we have a read mark
    let reads_before = read_table(&ctx.dir.path().join("reads"));
    assert_eq!(
        reads_before.len(),
        1,
        "expected 1 read mark before removal, got: {reads_before:?}"
    );

    // Now remove the feed
    ctx.run(&["feed", "rm", &url]).success();

    // Read marks for deleted posts should be cleaned up
    let reads_after = read_table(&ctx.dir.path().join("reads"));
    assert_eq!(
        reads_after.len(),
        0,
        "expected 0 read marks after feed removal (orphaned ReadMarks), got: {reads_after:?}"
    );
}

#[test]
fn test_feed_ls() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed1.xml");
    insert_feed(ctx.dir.path(), "https://example.com/feed2.xml");

    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = output.stdout_str();

    assert!(stdout.contains("https://example.com/feed1.xml"));
    assert!(stdout.contains("https://example.com/feed2.xml"));

    // Each line should start with a shorthand consisting only of home-row characters
    let home_row_chars: &[char] = &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'];
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
        assert!(
            first_word.starts_with('@'),
            "line should start with @shorthand: {}",
            line
        );
        let shorthand = &first_word[1..];
        assert!(
            !shorthand.is_empty(),
            "shorthand should not be empty: {}",
            line
        );
        assert!(
            shorthand.chars().all(|c| home_row_chars.contains(&c)),
            "shorthand '{}' contains non-home-row characters in line: {}",
            shorthand,
            line,
        );
    }
}

#[test]
fn test_feed_ls_no_feeds_prints_error() {
    let ctx = TestContext::new();

    let output = ctx.run(&["feed", "ls"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("No feeds found"),
        "expected 'No feeds found' on stderr, got: {}",
        stderr
    );
}

#[test]
fn test_trailing_slash_does_not_duplicate_feed() {
    let ctx = TestContext::new();

    let xml = rss_xml("Slash Blog", &[("Post", "Mon, 01 Jan 2024 00:00:00 +0000")]);
    ctx.mock_rss_feed("/feed.xml", &xml);
    ctx.mock_rss_feed("/feed.xml/", &xml);

    let url = ctx.server.url("/feed.xml");
    let url_slash = format!("{}/", url);

    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["feed", "add", &url_slash]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(
        feeds.len(),
        1,
        "trailing slash should not create a duplicate feed, got: {feeds:?}"
    );
}

#[test]
fn test_feed_remove_by_shorthand() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Keep Blog",
        &[("Keep Post", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-keep")],
    );
    ctx.mock_rss_feed("/keep.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Remove Blog",
        &[(
            "Remove Post",
            "Tue, 02 Jan 2024 00:00:00 +0000",
            "guid-remove",
        )],
    );
    ctx.mock_rss_feed("/remove.xml", &xml2);

    let keep_url = ctx.server.url("/keep.xml");
    let remove_url = ctx.server.url("/remove.xml");
    ctx.write_feeds(&[&keep_url, &remove_url]);
    ctx.run(&["sync"]).success();

    // Run feed ls and parse the shorthand for the remove_url
    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = output.stdout_str();

    let shorthand = stdout
        .lines()
        .find(|line| line.contains(&remove_url))
        .map(|line| {
            let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
            first_word // includes the '@' prefix
        })
        .expect("should find remove_url in feed ls output");

    // Remove using the shorthand
    ctx.run(&["feed", "rm", &shorthand]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), keep_url);

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["title"].as_str().unwrap(), "Keep Post");
}

#[test]
fn test_show_no_posts_prints_error() {
    let ctx = TestContext::new();

    let output = ctx.run(&["show"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("No matching posts"),
        "expected 'No matching posts' on stderr, got: {}",
        stderr
    );
}

#[test]
fn test_show_filter_by_shorthand() {
    let ctx = TestContext::new();

    let xml1 = rss_xml_with_guids(
        "Alpha Blog",
        &[
            ("Alpha Post 1", "Mon, 01 Jan 2024 00:00:00 +0000", "guid-a1"),
            ("Alpha Post 2", "Tue, 02 Jan 2024 00:00:00 +0000", "guid-a2"),
        ],
    );
    ctx.mock_rss_feed("/alpha.xml", &xml1);

    let xml2 = rss_xml_with_guids(
        "Beta Blog",
        &[("Beta Post 1", "Wed, 03 Jan 2024 00:00:00 +0000", "guid-b1")],
    );
    ctx.mock_rss_feed("/beta.xml", &xml2);

    let alpha_url = ctx.server.url("/alpha.xml");
    let beta_url = ctx.server.url("/beta.xml");
    ctx.write_feeds(&[&alpha_url, &beta_url]);
    ctx.run(&["sync"]).success();

    // Get shorthand for alpha feed
    let output = ctx.run(&["feed", "ls"]).success();
    let stdout = output.stdout_str();

    let alpha_shorthand = stdout
        .lines()
        .find(|line| line.contains(&alpha_url))
        .map(|line| {
            let first_word: String = line.chars().take_while(|c| *c != ' ').collect();
            first_word
        })
        .expect("should find alpha_url in feed ls output");

    // Filter with `show @shorthand` — should only show alpha posts
    let output = ctx.run(&["show", &alpha_shorthand]).success();
    let stdout = output.stdout_str();

    assert!(
        stdout.contains("Alpha Post 1"),
        "should contain Alpha Post 1"
    );
    assert!(
        stdout.contains("Alpha Post 2"),
        "should contain Alpha Post 2"
    );
    assert!(
        !stdout.contains("Beta Post 1"),
        "should NOT contain Beta Post 1"
    );

    // Also test with no subcommand: `blog @shorthand`
    let output = ctx.run(&[&alpha_shorthand]).success();
    let stdout = output.stdout_str();

    assert!(
        stdout.contains("Alpha Post 1"),
        "no-subcommand: should contain Alpha Post 1"
    );
    assert!(
        stdout.contains("Alpha Post 2"),
        "no-subcommand: should contain Alpha Post 2"
    );
    assert!(
        !stdout.contains("Beta Post 1"),
        "no-subcommand: should NOT contain Beta Post 1"
    );
}

#[test]
fn test_show_filter_unknown_shorthand() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed.xml");

    let output = ctx.run(&["show", "@zzz"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Unknown feed shorthand"),
        "expected unknown shorthand error, got: {}",
        stderr
    );
}

#[test]
fn test_remove_then_readd_feed() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Returning Blog",
        &[("Old Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/returning.xml", &xml);

    let url = ctx.server.url("/returning.xml");
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();
    ctx.run(&["feed", "rm", &url]).success();

    // Re-add and pull again
    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show", "2020-01-01.."]).success();
    let stdout = output.stdout_str();
    assert!(stdout.contains("Returning Blog"));
    assert!(stdout.contains("Old Post"));
}

#[test]
fn test_show_displays_post_shorthands() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(2);
    let date_b = recent_rss_date(3);
    let xml = rss_xml_with_guids(
        "Shorthand Blog",
        &[
            ("Post Alpha", &date_a, "guid-alpha"),
            ("Post Beta", &date_b, "guid-beta"),
        ],
    );
    ctx.mock_rss_feed("/shorthand.xml", &xml);

    let url = ctx.server.url("/shorthand.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["show", ".all", "2020-01-01.."]).success();
    let stdout = output.stdout_str();

    let post_alphabet: &[char] = &[
        'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L',
        'q', 'w', 'e', 'r', 't', 'y', 'i', 'o', 'p', 'z', 'x', 'c', 'v', 'b', 'n', 'm',
    ];

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // Lines are: "* YYYY-MM-DD  shorthand title (meta)"
        let words: Vec<&str> = line.split_whitespace().collect();
        assert!(
            words.len() >= 3,
            "line should have a marker, date, and shorthand: {}",
            line
        );
        let shorthand = words[2];
        assert!(
            shorthand.chars().all(|c| post_alphabet.contains(&c)),
            "shorthand '{}' should only contain POST_ALPHABET characters in line: {}",
            shorthand,
            line,
        );
    }
}

#[test]
fn test_open_unknown_shorthand() {
    let ctx = TestContext::new();

    let output = ctx.run(&["zzzzz", "open"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Unknown shorthand: zzzzz"),
        "expected 'Unknown shorthand: zzzzz' on stderr, got: {}",
        stderr,
    );
}

#[test]
fn test_open_valid_shorthand() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_links(
        "Open Blog",
        &[(
            "Open Post",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-open",
            "https://example.com/post/1",
        )],
    );
    ctx.mock_rss_feed("/open.xml", &xml);

    let url = ctx.server.url("/open.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Running `open a` should resolve the shorthand without error.
    // Use BROWSER=true to prevent actually opening a browser.
    let output = blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert();
    let stderr = output.stderr_str();
    assert!(
        !stderr.contains("Unknown shorthand"),
        "should resolve shorthand 'a', got: {}",
        stderr,
    );
    assert!(
        !stderr.contains("Post has no link"),
        "post should have a link, got: {}",
        stderr,
    );
}

#[test]
fn test_open_post_without_link() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"No Link Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"no-link-1","link":""}"#;
    std::fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    std::fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["a", "open"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Post has no link"),
        "expected 'Post has no link' on stderr, got: {}",
        stderr,
    );
}

#[test]
fn test_open_marks_post_as_read() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "Read Blog",
        &[
            ("Post A", &date_a, "guid-a", "https://example.com/a"),
            ("Post B", &date_b, "guid-b", "https://example.com/b"),
        ],
    );
    ctx.mock_rss_feed("/read.xml", &xml);
    let url = ctx.server.url("/read.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Before opening: both posts are unread (shown with *)
    let before = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(before.lines().filter(|l| l.starts_with('*')).count(), 2);

    // Open first post (shorthand "a")
    blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert()
        .success();

    // After opening: one post is read, one is still unread
    let after = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after.lines().filter(|l| l.starts_with('*')).count(),
        1,
        "expected 1 unread post after opening one, got:\n{after}"
    );
    assert_eq!(
        after.lines().filter(|l| l.starts_with("  ")).count(),
        1,
        "expected 1 read post after opening one, got:\n{after}"
    );
}

#[test]
fn test_remove_nonexistent_feed() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/keep.xml");

    let output = ctx
        .run(&["feed", "rm", "https://example.com/nonexistent.xml"])
        .failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Feed not found"),
        "expected 'Feed not found' on stderr, got: {}",
        stderr
    );

    // The existing feed should still be there
    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/keep.xml"
    );
}

#[test]
fn test_add_multiple_feeds() {
    let ctx = TestContext::new();

    let xml1 = rss_xml("Blog One", &[]);
    let xml2 = rss_xml("Blog Two", &[]);
    ctx.mock_rss_feed("/feed1.xml", &xml1);
    ctx.mock_rss_feed("/feed2.xml", &xml2);

    let url1 = ctx.server.url("/feed1.xml");
    let url2 = ctx.server.url("/feed2.xml");

    ctx.run(&["feed", "add", &url1, &url2]).success();

    let feeds = ctx.read_feeds();
    let urls: Vec<&str> = feeds.iter().map(|f| f["url"].as_str().unwrap()).collect();
    assert!(urls.iter().any(|u| *u == url1), "feed1 should be present");
    assert!(urls.iter().any(|u| *u == url2), "feed2 should be present");
    assert_eq!(feeds.len(), 2);
}

#[test]
fn test_add_multiple_feeds_stops_on_first_error() {
    let ctx = TestContext::new();

    let xml1 = rss_xml("Blog One", &[]);
    ctx.mock_rss_feed("/feed1.xml", &xml1);
    // /bad.xml is not mocked — will fail to resolve

    let url1 = ctx.server.url("/feed1.xml");
    let bad_url = ctx.server.url("/bad.xml");

    let output = ctx.run(&["feed", "add", &url1, &bad_url]).failure();
    let stderr = output.stderr_str();
    assert!(
        !stderr.is_empty(),
        "expected an error message for unresolvable feed, got empty stderr"
    );
}

#[test]
fn test_remove_multiple_feeds_by_url() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed1.xml");
    insert_feed(ctx.dir.path(), "https://example.com/feed2.xml");
    insert_feed(ctx.dir.path(), "https://example.com/keep.xml");

    ctx.run(&[
        "feed",
        "rm",
        "https://example.com/feed1.xml",
        "https://example.com/feed2.xml",
    ])
    .success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/keep.xml"
    );
}

#[test]
fn test_remove_multiple_feeds_stops_on_first_error() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed1.xml");
    insert_feed(ctx.dir.path(), "https://example.com/feed2.xml");

    let output = ctx
        .run(&[
            "feed",
            "rm",
            "https://example.com/feed1.xml",
            "https://example.com/nonexistent.xml",
            "https://example.com/feed2.xml",
        ])
        .failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("Feed not found"),
        "expected 'Feed not found' on stderr, got: {}",
        stderr
    );
}

#[test]
fn test_remove_multiple_feeds_by_shorthand() {
    let ctx = TestContext::new();

    insert_feed(ctx.dir.path(), "https://example.com/feed1.xml");
    insert_feed(ctx.dir.path(), "https://example.com/feed2.xml");
    insert_feed(ctx.dir.path(), "https://example.com/keep.xml");

    // Retrieve shorthands from feed ls output
    let ls_output = ctx.run(&["feed", "ls"]).success();
    let stdout = ls_output.stdout_str();

    // Parse shorthands — feed ls lines look like "  @sh  url"
    let shorthands: Vec<(&str, &str)> = stdout
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let at_pos = trimmed.find('@')?;
            let rest = &trimmed[at_pos + 1..];
            let (sh, url_part) = rest.split_once(|c: char| c.is_whitespace())?;
            let url = url_part.trim();
            Some((sh, url))
        })
        .collect();

    let sh1 = shorthands
        .iter()
        .find(|(_, u)| *u == "https://example.com/feed1.xml")
        .map(|(s, _)| *s)
        .expect("shorthand for feed1 not found");
    let sh2 = shorthands
        .iter()
        .find(|(_, u)| *u == "https://example.com/feed2.xml")
        .map(|(s, _)| *s)
        .expect("shorthand for feed2 not found");

    ctx.run(&["feed", "rm", &format!("@{sh1}"), &format!("@{sh2}")])
        .success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/keep.xml"
    );
}

#[test]
fn test_sync_continues_after_non_utf8_feed() {
    let ctx = TestContext::new();

    // Build an RSS feed with a non-UTF8 byte (0xe9 for Latin-1 'é') in the title
    let non_utf8_body: Vec<u8> = [
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
          <rss version=\"2.0\">\n\
            <channel>\n\
              <title>Caf"
            .as_slice(),
        &[0xe9],
        b"</title>\n\
              <item>\n\
                <title>Post</title>\n\
                <guid>urn:bad:1</guid>\n\
              </item>\n\
            </channel>\n\
          </rss>"
            .as_slice(),
    ]
    .concat();

    ctx.mock_rss_feed_bytes("/bad.xml", &non_utf8_body);

    // A second feed that is valid
    let good_xml = rss_xml(
        "Good Blog",
        &[("Good Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/good.xml", &good_xml);

    let bad_url = ctx.server.url("/bad.xml");
    let good_url = ctx.server.url("/good.xml");
    ctx.write_feeds(&[&bad_url, &good_url]);

    // Pull should succeed overall — the bad feed errors but doesn't crash
    ctx.run(&["sync"]).success();

    // At minimum the good feed's post should be present
    let posts = ctx.read_posts();
    assert!(
        posts
            .iter()
            .any(|p| p["title"].as_str() == Some("Good Post")),
        "good feed's post should be present"
    );
}

#[test]
fn test_atom_feed_with_rss_in_content_is_parsed_as_atom() {
    let ctx = TestContext::new();

    // An Atom feed whose entry summary uses CDATA containing the literal "<rss" —
    // the naive `text.contains("<rss")` check misidentifies this as RSS.
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Blog</title>
  <id>urn:atom-blog</id>
  <updated>2024-01-01T00:00:00Z</updated>
  <entry>
    <title>How to migrate to Atom</title>
    <id>urn:post:1</id>
    <updated>2024-01-01T00:00:00Z</updated>
    <published>2024-01-01T00:00:00Z</published>
    <summary type="html"><![CDATA[Replace <rss version="2.0"> with Atom]]></summary>
  </entry>
</feed>"#;
    ctx.mock_atom_feed("/atom-with-rss-mention.xml", xml);

    let url = ctx.server.url("/atom-with-rss-mention.xml");
    ctx.write_feeds(&[&url]);

    ctx.run(&["sync"]).success();

    let posts = ctx.read_posts();
    assert_eq!(posts.len(), 1);
    assert_eq!(
        posts[0]["title"].as_str().unwrap(),
        "How to migrate to Atom"
    );
}

// --- Sync integration tests ---

/// Helper to run a git command in a directory.
/// Git env vars injected by git when running hooks. Any subprocess that does
/// git operations must clear these or it will accidentally operate on the host
/// repo's index instead of its own temp directory.
const GIT_HOOK_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

fn git_cmd() -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    for var in GIT_HOOK_VARS {
        cmd.env_remove(var);
    }
    cmd
}

#[allow(deprecated)]
fn blog_cmd() -> assert_cmd::Command {
    let mut cmd = Command::cargo_bin("blog").unwrap();
    for var in GIT_HOOK_VARS {
        cmd.env_remove(var);
    }
    cmd
}

fn git(dir: &Path, args: &[&str]) {
    let output = git_cmd()
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Convert a filesystem path to a proper file:// URL.
/// On Windows, backslashes are replaced with forward slashes and the path
/// is prefixed with an extra `/` so `C:\foo` becomes `file:///C:/foo`.
fn path_to_file_url(path: &Path) -> String {
    let s = path.display().to_string().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn git_config_test_user(dir: &Path) {
    git(dir, &["config", "user.name", "Test"]);
    git(dir, &["config", "user.email", "test@test.com"]);
}

/// Write a feed URL directly to the store, bypassing the CLI.
/// Also commits to git if the store directory is a git repository.
fn insert_feed(store_dir: &Path, url: &str) {
    let feeds_dir = store_dir.join("feeds");
    fs::create_dir_all(&feeds_dir).unwrap();
    let file_path = feeds_dir.join("items_.jsonl");
    // Replicate synctato's hash_id: SHA-256 of the key, first 11 hex chars
    // (id_length_for_capacity(50_000) == 11 for FeedSource)
    let digest = Sha256::digest(url.as_bytes());
    let hash: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    let id = &hash[..11];
    let entry = serde_json::json!({
        "id": id,
        "url": url,
        "title": "",
        "site_url": "",
        "description": "",
        "is_fetched": false
    });
    {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .unwrap();
        writeln!(file, "{}", entry).unwrap();
    } // file is dropped/flushed here before git add

    let git_check = git_cmd()
        .args(["-C", &store_dir.to_string_lossy(), "rev-parse", "--git-dir"])
        .output();
    match git_check {
        Ok(output) if output.status.success() => {
            git(store_dir, &["add", "feeds/"]);
            git(store_dir, &["commit", "-m", &format!("add feed: {url}")]);
        }
        _ => {}
    }
}

/// Initialize a git repo, configure user, add remote, and make initial commit.
fn init_git_store(store_dir: &Path, origin_dir: &Path) {
    git(store_dir, &["init"]);
    git_config_test_user(store_dir);
    git(
        store_dir,
        &["remote", "add", "origin", &path_to_file_url(origin_dir)],
    );
    // Make an initial commit so we have HEAD
    fs::write(store_dir.join(".keep"), "").unwrap();
    git(store_dir, &["add", "."]);
    git(store_dir, &["commit", "-m", "init"]);
    git(store_dir, &["push", "-u", "origin", "HEAD"]);
}

/// Clone from a bare origin into a new temp dir, return its path.
fn clone_store(origin_dir: &Path) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let output = git_cmd()
        .args([
            "clone",
            &path_to_file_url(origin_dir),
            &dir.path().to_string_lossy(),
        ])
        .output()
        .expect("failed to clone");
    assert!(
        output.status.success(),
        "clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git_config_test_user(dir.path());
    let p = dir.path().to_path_buf();
    (dir, p)
}

fn run_blog(store_dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    blog_cmd().args(args).env("RSS_STORE", store_dir).assert()
}

#[test]
fn test_sync_no_remote_warns() {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init"]);
    git_config_test_user(dir.path());

    let output = run_blog(dir.path(), &["sync"]).success();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("no remote"),
        "expected 'no remote' warning, got: {}",
        stderr
    );
}

#[test]
fn test_sync_dirty_repo_fails() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Make it dirty with a data file (use a dir the store doesn't read)
    fs::create_dir_all(store_dir.path().join("extra")).unwrap();
    fs::write(store_dir.path().join("extra/items_00.jsonl"), "dirty").unwrap();

    let output = run_blog(store_dir.path(), &["sync"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("uncommitted"),
        "expected 'uncommitted' error, got: {}",
        stderr
    );
}

#[test]
fn test_sync_first_push() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    // Init repo but don't push yet (no remote branch)
    git(store_dir.path(), &["init"]);
    git_config_test_user(store_dir.path());
    git(
        store_dir.path(),
        &[
            "remote",
            "add",
            "origin",
            &path_to_file_url(origin_dir.path()),
        ],
    );

    // Add a feed (insert_feed auto-commits since git repo exists)
    insert_feed(store_dir.path(), "https://example.com/feed.xml");

    // Sync should push
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify we can clone and see the data
    let (clone_td, clone_dir) = clone_store(origin_dir.path());
    let feeds = read_table(&clone_dir.join("feeds"));
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/feed.xml"
    );
    drop(clone_td);
}

#[test]
fn test_sync_local_ahead_only() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add feed locally (insert_feed auto-commits)
    insert_feed(store_dir.path(), "https://example.com/a.xml");

    // Sync should push
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify
    let (clone_td, clone_dir) = clone_store(origin_dir.path());
    let feeds = read_table(&clone_dir.join("feeds"));
    assert!(
        feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/a.xml")),
        "remote should have the feed after sync"
    );
    drop(clone_td);
}

#[test]
fn test_sync_no_remote_still_fetches_posts() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml(
        "Test Feed",
        &[("Test Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    // Initial git repo, like init_git_store without adding remote
    git(dir.path(), &["init"]);
    git_config_test_user(dir.path());
    fs::write(dir.path().join(".keep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "init"]);

    insert_feed(dir.path(), &url);
    run_blog(dir.path(), &["sync", "--no-remote"]).success();

    let posts = read_table(&dir.path().join("posts"));
    assert!(
        posts
            .iter()
            .any(|p| p["title"].as_str() == Some("Test Post")),
        "--no-remote should still fetch posts locally, got: {posts:?}"
    );
}

#[test]
fn test_sync_no_remote_does_not_pull_remote_feed() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Remote device adds a feed and pushes
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, "https://example.com/remote.xml");
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Sync without remote should NOT merge in the remote feed
    run_blog(store_dir.path(), &["sync", "--no-remote"]).success();

    // Verify
    let feeds = read_table(&store_dir.path().join("feeds"));
    assert!(
        !feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/remote.xml")),
        "--no-remote should not pull the remote feed, got: {feeds:?}"
    );
}

#[test]
fn test_sync_no_remote_does_not_push() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add a feed locally (auto-committed)
    insert_feed(store_dir.path(), "https://example.com/a.xml");

    // Sync with --no-remote should NOT push local changes
    run_blog(store_dir.path(), &["sync", "--no-remote"]).success();

    // Verify
    let (clone_td, clone_dir) = clone_store(origin_dir.path());
    let feeds = read_table(&clone_dir.join("feeds"));
    assert!(
        !feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/a.xml")),
        "--no-remote should not push the local feed to remote, got: {feeds:?}"
    );
    drop(clone_td);
}

#[test]
fn test_sync_remote_ahead_only() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Create a second clone, add feed there (auto-committed), push
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, "https://example.com/remote.xml");
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Local sync should merge the remote feed
    run_blog(store_dir.path(), &["sync"]).success();

    let feeds = read_table(&store_dir.path().join("feeds"));
    assert!(
        feeds
            .iter()
            .any(|f| f["url"].as_str() == Some("https://example.com/remote.xml")),
        "local should have remote feed after sync"
    );
}

#[test]
fn test_sync_both_diverged() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add feed on remote side (auto-committed), push
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, "https://example.com/b.xml");
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Add feed on local side (diverged, auto-committed)
    insert_feed(store_dir.path(), "https://example.com/a.xml");

    // Sync merges both
    run_blog(store_dir.path(), &["sync"]).success();

    let feeds = read_table(&store_dir.path().join("feeds"));
    let urls: Vec<&str> = feeds.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(
        urls.contains(&"https://example.com/a.xml"),
        "should have local feed A"
    );
    assert!(
        urls.contains(&"https://example.com/b.xml"),
        "should have remote feed B"
    );
}

#[test]
fn test_sync_two_way() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    // Clone 1
    let store1 = TempDir::new().unwrap();
    init_git_store(store1.path(), origin_dir.path());

    // Clone 2
    let (store2_td, store2_dir) = clone_store(origin_dir.path());

    // Clone 1 adds feed A (auto-committed) and syncs
    insert_feed(store1.path(), "https://example.com/a.xml");
    run_blog(store1.path(), &["sync"]).success();

    // Clone 2 adds feed B (auto-committed) and syncs
    insert_feed(&store2_dir, "https://example.com/b.xml");
    run_blog(&store2_dir, &["sync"]).success();

    // Clone 1 syncs again to pick up B
    run_blog(store1.path(), &["sync"]).success();

    // Both should have A and B
    let feeds1 = read_table(&store1.path().join("feeds"));
    let urls1: Vec<&str> = feeds1.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(urls1.contains(&"https://example.com/a.xml"));
    assert!(urls1.contains(&"https://example.com/b.xml"));

    let feeds2 = read_table(&store2_dir.join("feeds"));
    let urls2: Vec<&str> = feeds2.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(urls2.contains(&"https://example.com/a.xml"));
    assert!(urls2.contains(&"https://example.com/b.xml"));

    drop(store2_td);
}

#[test]
fn test_git_passthrough() {
    let dir = TempDir::new().unwrap();

    let output = run_blog(dir.path(), &["git", "init"]).success();
    let _ = output;

    // Now git status should work
    run_blog(dir.path(), &["git", "status"]).success();
}

#[test]
fn test_git_remote_add() {
    let dir = TempDir::new().unwrap();
    run_blog(dir.path(), &["git", "init"]).success();
    run_blog(
        dir.path(),
        &[
            "git",
            "remote",
            "add",
            "origin",
            "https://example.com/repo.git",
        ],
    )
    .success();

    // Verify remote was added
    let output = git_cmd()
        .args([
            "-C",
            &dir.path().to_string_lossy(),
            "remote",
            "get-url",
            "origin",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let url = String::from_utf8_lossy(&output.stdout);
    assert!(url.trim() == "https://example.com/repo.git");
}

#[test]
fn test_transact_auto_commits_with_existing_repo() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    git(dir.path(), &["init"]);
    git_config_test_user(dir.path());
    // Initial commit so HEAD exists
    fs::write(dir.path().join(".keep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "init"]);

    // Add a feed — should auto-commit because git repo exists
    run_blog(dir.path(), &["feed", "add", &url]).success();

    // Check that a commit was made
    let output = git_cmd()
        .args([
            "-C",
            &dir.path().to_string_lossy(),
            "log",
            "--oneline",
            "-1",
        ])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stdout);
    assert!(
        log.contains("add feed"),
        "commit message should contain 'add feed', got: {}",
        log
    );
}

#[test]
fn test_transact_no_git_repo_still_works() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    // No git init — just add a feed
    run_blog(dir.path(), &["feed", "add", &url]).success();

    // Feed should be saved
    let feeds = read_table(&dir.path().join("feeds"));
    assert_eq!(feeds.len(), 1);
}

#[test]
fn test_transact_dirty_repo_fails() {
    let dir = TempDir::new().unwrap();
    let server = MockServer::start();
    let xml = rss_xml("Test Feed", &[]);
    server.mock(|when, then| {
        when.method(GET).path("/feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });
    let url = server.url("/feed.xml");

    git(dir.path(), &["init"]);
    git_config_test_user(dir.path());
    fs::write(dir.path().join(".keep"), "").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-m", "init"]);

    // Make dirty with a data file (use a dir the store doesn't read)
    fs::create_dir_all(dir.path().join("extra")).unwrap();
    fs::write(dir.path().join("extra/items_00.jsonl"), "dirty").unwrap();

    let output = run_blog(dir.path(), &["feed", "add", &url]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("uncommitted"),
        "expected 'uncommitted' error, got: {}",
        stderr
    );
}

fn commit_count(dir: &Path) -> usize {
    let output = git_cmd()
        .args(["-C", &dir.to_string_lossy(), "rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn test_sync_already_in_sync_creates_no_commits() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add a feed and sync
    insert_feed(store_dir.path(), "https://example.com/feed.xml");
    run_blog(store_dir.path(), &["sync"]).success();

    let commits_before = commit_count(store_dir.path());

    // Sync again — nothing changed, should not create any new commits
    run_blog(store_dir.path(), &["sync"]).success();

    let commits_after = commit_count(store_dir.path());
    assert_eq!(
        commits_before, commits_after,
        "sync with no changes should not create new commits"
    );
}

#[test]
fn test_sync_no_git_repo() {
    // sync should work with no git repo at all (pure feed pulling)
    let dir = TempDir::new().unwrap();
    insert_feed(dir.path(), "https://example.com/feed.xml");
    run_blog(dir.path(), &["sync"]).success();

    let feeds = read_table(&dir.path().join("feeds"));
    assert_eq!(feeds.len(), 1);
}

#[test]
fn test_sync_local_ahead_pushes_without_merge() {
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    let store_dir = TempDir::new().unwrap();
    init_git_store(store_dir.path(), origin_dir.path());

    // Add two feeds locally (auto-committed each time)
    insert_feed(store_dir.path(), "https://example.com/a.xml");
    insert_feed(store_dir.path(), "https://example.com/b.xml");

    // Sync should just push (no merge commit)
    run_blog(store_dir.path(), &["sync"]).success();

    // Verify no merge commits (all commits should have at most 1 parent)
    let output = git_cmd()
        .args([
            "-C",
            &store_dir.path().to_string_lossy(),
            "log",
            "--format=%P",
        ])
        .output()
        .unwrap();
    let parents = String::from_utf8_lossy(&output.stdout);
    for line in parents.lines() {
        let parent_count = line.split_whitespace().count();
        assert!(
            parent_count <= 1,
            "expected no merge commits, but found a commit with {} parents",
            parent_count
        );
    }
}

#[test]
fn test_pull_command_removed() {
    let dir = TempDir::new().unwrap();
    run_blog(dir.path(), &["pull"]).failure();
}

#[test]
fn test_add_direct_feed_url_still_works() {
    let ctx = TestContext::new();
    let xml = rss_xml(
        "Direct Feed",
        &[("Post One", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);

    let url = ctx.server.url("/feed.xml");
    ctx.run(&["feed", "add", &url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), url);
}

#[test]
fn test_add_html_page_discovers_feed() {
    let ctx = TestContext::new();

    let feed_xml = rss_xml(
        "Discovered Feed",
        &[("Discovered Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &feed_xml);

    let feed_url = ctx.server.url("/feed.xml");
    let html = format!(
        r#"<html><head>
        <link rel="alternate" type="application/rss+xml" href="{feed_url}" title="My Blog Feed">
        </head><body><p>Hello</p></body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

#[test]
fn test_add_html_page_multiple_feeds_fails() {
    let ctx = TestContext::new();

    let feed1_url = ctx.server.url("/feed1.xml");
    let feed2_url = ctx.server.url("/feed2.xml");
    let html = format!(
        r#"<html><head>
        <link rel="alternate" type="application/rss+xml" href="{feed1_url}" title="Feed 1">
        <link rel="alternate" type="application/atom+xml" href="{feed2_url}" title="Feed 2">
        </head><body></body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).failure();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 0);
}

#[test]
fn test_add_html_page_no_feeds_fails() {
    let ctx = TestContext::new();

    let html = r#"<html><head><title>No Feeds</title></head><body><p>Hello</p></body></html>"#;
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).failure();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 0);
}

#[test]
fn test_add_html_page_ignores_non_feed_candidates() {
    let ctx = TestContext::new();

    // Real feed linked only via <a> tag (no <link rel="alternate">)
    let feed_xml = rss_xml(
        "Real Feed",
        &[("A Post", "Mon, 01 Jan 2024 00:00:00 +0000")],
    );
    ctx.mock_rss_feed("/feed.xml", &feed_xml);

    // Non-feed URL with "feed" in the path — feedfinder picks it up from <a> tags
    ctx.server.mock(|when, then| {
        when.method(GET).path("/buzzfeed");
        then.status(200)
            .header("Content-Type", "text/html")
            .body("<html><body>not a feed</body></html>");
    });

    // No <link> tags — feedfinder falls through to <a> tag scanning,
    // which matches both URLs because they contain "feed" in the href
    let feed_url = ctx.server.url("/feed.xml");
    let not_a_feed_url = ctx.server.url("/buzzfeed");
    let html = format!(
        r#"<html><head><title>Blog</title></head><body>
        <a href="{feed_url}">RSS feed</a>
        <a href="{not_a_feed_url}">BuzzFeed article</a>
        </body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

#[test]
fn test_add_html_page_deduplicates_feed_candidates() {
    let ctx = TestContext::new();

    let feed_xml = rss_xml("My Feed", &[("A Post", "Mon, 01 Jan 2024 00:00:00 +0000")]);
    ctx.mock_rss_feed("/index.xml", &feed_xml);

    // Same feed URL appears in multiple <a> tags (e.g., header + footer)
    // No <link> tags, so feedfinder falls through to <a> scanning
    let feed_url = ctx.server.url("/index.xml");
    let html = format!(
        r#"<html><head><title>Blog</title></head><body>
        <nav><a href="{feed_url}">RSS</a></nav>
        <article><p>Content</p></article>
        <footer><a href="{feed_url}">RSS feed</a></footer>
        </body></html>"#
    );
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    let blog_url = ctx.server.url("/blog");
    ctx.run(&["feed", "add", &blog_url]).success();

    let feeds = ctx.read_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0]["url"].as_str().unwrap(), feed_url);
}

#[test]
fn test_add_html_page_caps_candidate_validation() {
    let ctx = TestContext::new();

    // Create 25 valid RSS feeds — more than the cap of 20
    let feed_xml = rss_xml("Feed", &[("Post", "Mon, 01 Jan 2024 00:00:00 +0000")]);
    let mut link_tags = String::new();
    for i in 0..25 {
        let path = format!("/feed{i}.xml");
        ctx.mock_rss_feed(&path, &feed_xml);
        let url = ctx.server.url(&path);
        link_tags.push_str(&format!(
            r#"<link rel="alternate" type="application/rss+xml" href="{url}" title="Feed {i}">"#
        ));
    }

    let html = format!(r#"<html><head>{link_tags}</head><body><p>Hello</p></body></html>"#);
    ctx.server.mock(|when, then| {
        when.method(GET).path("/blog");
        then.status(200)
            .header("Content-Type", "text/html")
            .body(&html);
    });

    // With 25 valid feeds discovered, the command should fail (multiple feeds found).
    // The key assertion: it should report at most 20 candidates, not all 25.
    let blog_url = ctx.server.url("/blog");
    let output = ctx.run(&["feed", "add", &blog_url]).failure();
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Count how many feed URLs appear in the "Multiple feeds found" listing
    let listed_feeds = stderr.lines().filter(|l| l.contains("/feed")).count();
    assert!(
        listed_feeds <= 20,
        "Expected at most 20 validated candidates, but found {listed_feeds}"
    );
}

// --- clone command tests ---

#[test]
fn test_clone_into_empty_dir() {
    // Set up a bare origin repo with some feed data
    let origin_dir = TempDir::new().unwrap();
    git_cmd()
        .args(["init", "--bare"])
        .arg(origin_dir.path())
        .output()
        .unwrap();

    // Create a temporary working repo, add data, push to origin
    let work_dir = TempDir::new().unwrap();
    git_cmd()
        .args(["clone", &path_to_file_url(origin_dir.path())])
        .arg(work_dir.path())
        .output()
        .unwrap();
    git_config_test_user(work_dir.path());

    insert_feed(work_dir.path(), "https://example.com/feed.xml");

    git(work_dir.path(), &["push", "origin", "HEAD"]);

    // Clone into a fresh store dir using the blog clone command
    let store_dir = TempDir::new().unwrap();
    let target = store_dir.path().join("store");

    blog_cmd()
        .args(["clone", &path_to_file_url(origin_dir.path())])
        .env("RSS_STORE", &target)
        .assert()
        .success();

    // Verify feed data is present
    let feeds = read_table(&target.join("feeds"));
    assert_eq!(feeds.len(), 1);
    assert_eq!(
        feeds[0]["url"].as_str().unwrap(),
        "https://example.com/feed.xml"
    );
}

#[test]
fn test_clone_merges_with_existing_store() {
    // Set up a "remote" bare repo with feed B
    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    // Create a temporary store, add feed B, push to origin
    let remote_store = TempDir::new().unwrap();
    init_git_store(remote_store.path(), origin_dir.path());
    insert_feed(remote_store.path(), "https://example.com/b.xml");
    git(remote_store.path(), &["push", "-u", "origin", "HEAD"]);
    drop(remote_store);

    // Create local store with feed A (independent git history, no remote)
    let local_store = TempDir::new().unwrap();
    git(local_store.path(), &["init"]);
    git_config_test_user(local_store.path());
    insert_feed(local_store.path(), "https://example.com/a.xml");

    // Clone into existing store — should add remote and merge
    run_blog(
        local_store.path(),
        &["clone", &path_to_file_url(origin_dir.path())],
    )
    .success();

    // Both feeds must survive
    let feeds = read_table(&local_store.path().join("feeds"));
    let urls: Vec<&str> = feeds.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(
        urls.contains(&"https://example.com/a.xml"),
        "local feed A should survive merge, got: {urls:?}"
    );
    assert!(
        urls.contains(&"https://example.com/b.xml"),
        "remote feed B should be merged in, got: {urls:?}"
    );
}

// --- Date filtering integration tests ---

#[test]
fn test_show_since_filters_posts() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Old Post","date":"2024-01-14T00:00:00Z","feed":"Alice","raw_id":"old1","link":""}
{"id":"2","title":"New Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"new1","link":""}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    insert_feed(ctx.dir.path(), "https://example.com/alice.xml");

    let output = ctx.run(&["show", "2024-01-15.."]).success();
    let stdout = output.stdout_str();

    assert!(
        !stdout.contains("Old Post"),
        "Old Post should be filtered out by 2024-01-15.."
    );
    assert!(
        stdout.contains("New Post"),
        "New Post should be shown with 2024-01-15.."
    );
}

#[test]
fn test_show_until_filters_posts() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Old Post","date":"2024-01-14T00:00:00Z","feed":"Alice","raw_id":"old1","link":""}
{"id":"2","title":"New Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"new1","link":""}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    insert_feed(ctx.dir.path(), "https://example.com/alice.xml");

    let output = ctx.run(&["show", "..2024-01-14"]).success();
    let stdout = output.stdout_str();

    assert!(
        stdout.contains("Old Post"),
        "Old Post should be shown with ..2024-01-14"
    );
    assert!(
        !stdout.contains("New Post"),
        "New Post should be filtered out by ..2024-01-14"
    );
}

#[test]
fn test_show_date_range_combined() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Old Post","date":"2024-01-10T00:00:00Z","feed":"Alice","raw_id":"old1","link":""}
{"id":"2","title":"Mid Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"mid1","link":""}
{"id":"3","title":"New Post","date":"2024-01-20T00:00:00Z","feed":"Alice","raw_id":"new1","link":""}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    insert_feed(ctx.dir.path(), "https://example.com/alice.xml");

    let output = ctx.run(&["show", "2024-01-14..2024-01-16"]).success();
    let stdout = output.stdout_str();

    assert!(
        !stdout.contains("Old Post"),
        "Old Post should be filtered out"
    );
    assert!(
        stdout.contains("Mid Post"),
        "Mid Post should be shown in range"
    );
    assert!(
        !stdout.contains("New Post"),
        "New Post should be filtered out"
    );
}

#[test]
fn test_show_range_with_grouping() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Old Post","date":"2024-01-10T00:00:00Z","feed":"Alice","raw_id":"old1","link":""}
{"id":"2","title":"New Post","date":"2024-01-15T00:00:00Z","feed":"Alice","raw_id":"new1","link":""}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    insert_feed(ctx.dir.path(), "https://example.com/alice.xml");

    let output = ctx.run(&["show", "/d", "2024-01-15.."]).success();
    let stdout = output.stdout_str();

    assert!(
        stdout.contains("=== 2024-01-15 ==="),
        "Should show date group header for 2024-01-15"
    );
    assert!(
        !stdout.contains("Old Post"),
        "Old Post should be filtered out"
    );
    assert!(stdout.contains("New Post"), "New Post should be shown");
}

#[test]
fn test_unread_command() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "Unread Blog",
        &[
            ("Post A", &date_a, "guid-a", "https://example.com/a"),
            ("Post B", &date_b, "guid-b", "https://example.com/b"),
        ],
    );
    ctx.mock_rss_feed("/unread.xml", &xml);
    let url = ctx.server.url("/unread.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Before opening: both posts are unread (shown with *)
    let before = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(before.lines().filter(|l| l.starts_with('*')).count(), 2);

    // Open first post (shorthand "a") to mark it read
    blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert()
        .success();

    // After opening: one post is read, one is still unread
    let after_open = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after_open.lines().filter(|l| l.starts_with('*')).count(),
        1,
        "expected 1 unread post after opening one, got:\n{after_open}"
    );

    // Mark it unread again
    ctx.run(&["a", "unread"]).success();

    // After unread: both posts should be unread again
    let after_unread = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after_unread.lines().filter(|l| l.starts_with('*')).count(),
        2,
        "expected 2 unread posts after marking one unread, got:\n{after_unread}"
    );
}

#[test]
fn test_target_first_open() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "Target First Blog",
        &[
            ("Post A", &date_a, "guid-a", "https://example.com/a"),
            ("Post B", &date_b, "guid-b", "https://example.com/b"),
        ],
    );
    ctx.mock_rss_feed("/tf.xml", &xml);
    let url = ctx.server.url("/tf.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Both posts are unread
    let before = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(before.lines().filter(|l| l.starts_with('*')).count(), 2);

    // Use target-first syntax: `a open` instead of `open a`
    blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert()
        .success();

    // After opening: one post is read
    let after = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after.lines().filter(|l| l.starts_with('*')).count(),
        1,
        "expected 1 unread post after target-first open, got:\n{after}"
    );
}

#[test]
fn test_target_first_read() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_links(
        "Target First Read Blog",
        &[(
            "Post A",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-a",
            "https://example.com/a",
        )],
    );
    ctx.mock_rss_feed("/tfr.xml", &xml);
    let url = ctx.server.url("/tfr.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Use target-first syntax: `a read` instead of `read a`
    let output = ctx.run(&["a", "read"]).success().stdout_str();
    assert!(
        output.contains("https://example.com/a"),
        "expected URL in output, got: {output}"
    );
}

#[test]
fn test_target_first_unread() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_links(
        "Target First Unread Blog",
        &[(
            "Post A",
            "Mon, 01 Jan 2024 00:00:00 +0000",
            "guid-a",
            "https://example.com/a",
        )],
    );
    ctx.mock_rss_feed("/tfu.xml", &xml);
    let url = ctx.server.url("/tfu.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Mark as read first via target-first open
    blog_cmd()
        .args(["a", "open"])
        .env("RSS_STORE", ctx.dir.path())
        .env("BROWSER", "true")
        .assert()
        .success();

    let after_open = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after_open.lines().filter(|l| l.starts_with('*')).count(),
        0,
        "expected 0 unread posts after open"
    );

    // Use target-first syntax: `a unread` instead of `unread a`
    ctx.run(&["a", "unread"]).success();

    let after_unread = ctx.run(&["show", "2020-01-01.."]).success().stdout_str();
    assert_eq!(
        after_unread.lines().filter(|l| l.starts_with('*')).count(),
        1,
        "expected 1 unread post after target-first unread, got:\n{after_unread}"
    );
}

#[test]
fn test_show_default_query_hides_read_posts() {
    let ctx = TestContext::new();

    // Use recent dates so the 3-month window doesn't filter them out
    let now = chrono::Utc::now();
    let recent = now - chrono::Duration::days(1);
    let date_str = recent.format("%a, %d %b %Y %H:%M:%S +0000").to_string();

    let xml = rss_xml_with_links(
        "Default Query Blog",
        &[(
            "Recent Post",
            &date_str,
            "guid-recent",
            "https://example.com/recent",
        )],
    );
    ctx.mock_rss_feed("/default.xml", &xml);
    let url = ctx.server.url("/default.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Default show (no args) should display the unread post
    let before = ctx.run(&[]).success().stdout_str();
    assert!(
        before.contains("Recent Post"),
        "unread post should appear by default"
    );

    // Mark it read
    ctx.run(&["a", "read"]).success();

    // Default show should now hide the read post
    let after = ctx.run(&[]);
    let after = after.failure();
    let stderr = after.stderr_str();
    assert!(
        stderr.contains("No matching posts"),
        "read posts should be hidden by default, got: {stderr}"
    );
}

#[test]
fn test_show_default_query_hides_old_posts() {
    let ctx = TestContext::new();

    // One old post (> 3 months) and one recent post
    let posts = format!(
        r#"{{"id":"old","title":"Old Post","date":"2020-01-15T00:00:00Z","feed":"Alice"}}
{{"id":"new","title":"New Post","date":"{}","feed":"Alice"}}"#,
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    );
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), &posts).unwrap();

    // Default show should only display the recent post
    let output = ctx.run(&[]).success().stdout_str();
    assert!(output.contains("New Post"), "recent post should appear");
    assert!(
        !output.contains("Old Post"),
        "old post should be hidden by default"
    );
}

#[test]
fn test_show_default_query_groups_by_week() {
    let ctx = TestContext::new();

    let now = chrono::Utc::now();
    let date = now - chrono::Duration::days(1);
    let posts = format!(
        r#"{{"id":"1","title":"Post A","date":"{}","feed":"Alice"}}"#,
        date.format("%Y-%m-%dT%H:%M:%SZ")
    );
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), &posts).unwrap();

    // Default show should group by week (=== YYYY-Www ===)
    let output = ctx.run(&[]).success().stdout_str();
    assert!(
        output.lines().any(|l| l.contains("-W")),
        "default show should group by week, got:\n{output}"
    );
}

#[test]
fn test_show_all_bypasses_default_query() {
    let ctx = TestContext::new();

    // Old post that the default query would hide
    let posts = r#"{"id":"1","title":"Old Post","date":"2020-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    // Default show hides it (too old)
    let default_output = ctx.run(&[]);
    default_output.failure();

    // .all bypasses the default and shows everything
    let output = ctx.run(&[".all"]).success().stdout_str();
    assert!(
        output.contains("Old Post"),
        ".all should show old posts, got:\n{output}"
    );
}

#[test]
fn test_export_outputs_jsonl() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "Export Blog",
        &[
            ("Post A", &date_a, "guid-a", "https://example.com/a"),
            ("Post B", &date_b, "guid-b", "https://example.com/b"),
        ],
    );
    ctx.mock_rss_feed("/export.xml", &xml);
    let url = ctx.server.url("/export.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    let output = ctx.run(&[".all", "export"]).success().stdout_str();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 JSONL lines, got:\n{output}");

    // Each line should be valid JSON with id and expanded feed object
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert!(parsed.get("id").is_some(), "export should include id field");
    assert!(
        !parsed["id"].as_str().unwrap().is_empty(),
        "id should be non-empty"
    );
    assert!(parsed.get("title").is_some());
    assert!(parsed.get("date").is_some());
    let feed = parsed.get("feed").expect("feed field should exist");
    assert!(feed.is_object(), "feed should be an object, got: {feed}");
    assert!(feed.get("url").is_some(), "feed should have url field");

    // Unread posts should not have read_at
    assert!(
        parsed.get("read_at").is_none(),
        "unread post should not have read_at"
    );

    // Mark first post as read, then export again
    ctx.run(&["a", "read"]).success();
    let output2 = ctx.run(&[".all", "export"]).success().stdout_str();
    let lines2: Vec<&str> = output2.lines().collect();
    // Find the read post (Post A)
    let read_line = lines2.iter().find(|l| l.contains("Post A")).unwrap();
    let read_parsed: serde_json::Value = serde_json::from_str(read_line).unwrap();
    assert!(
        read_parsed.get("read_at").is_some(),
        "read post should have read_at"
    );

    // Unread post should still not have read_at
    let unread_line = lines2.iter().find(|l| l.contains("Post B")).unwrap();
    let unread_parsed: serde_json::Value = serde_json::from_str(unread_line).unwrap();
    assert!(
        unread_parsed.get("read_at").is_none(),
        "unread post should not have read_at"
    );
}

#[test]
fn test_export_respects_filters() {
    let ctx = TestContext::new();

    let xml = rss_xml_with_links(
        "Filter Blog",
        &[
            (
                "Post A",
                "Mon, 15 Jan 2024 00:00:00 +0000",
                "guid-a",
                "https://example.com/a",
            ),
            (
                "Post B",
                "Sun, 14 Jan 2024 00:00:00 +0000",
                "guid-b",
                "https://example.com/b",
            ),
        ],
    );
    ctx.mock_rss_feed("/filter.xml", &xml);
    let url = ctx.server.url("/filter.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    let output = ctx.run(&["export", "2024-01-15.."]).success().stdout_str();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected 1 filtered JSONL line, got:\n{output}"
    );
    assert!(lines[0].contains("Post A"));
}

#[test]
fn test_invalid_ingest_filter_returns_error_on_sync() {
    let ctx = TestContext::new();
    ctx.run(&["config", "set", "ingest_filter", "[invalid jq"])
        .success();

    let xml = rss_xml_with_links(
        "Blog",
        &[("Post", &recent_rss_date(1), "g1", "https://example.com")],
    );
    ctx.mock_rss_feed("/feed.xml", &xml);
    let url = ctx.server.url("/feed.xml");
    ctx.write_feeds(&[&url]);

    let err = ctx.run(&["sync"]).failure().stderr_str();
    assert!(err.contains("jq"), "got: {err}");
}

/// Parametric test covering all ingest_filter examples from the README.
/// Each case uses the same feed fixture with a real post, a sponsored post,
/// a partner content post, and tracking parameters on all links.
#[rstest]
#[case::filter_sponsored(
    r#"[.[] | select(.title | startswith("[Sponsored]") or contains("Partner Content") | not)]"#,
    &["Linux 7.0"],
    &["Sponsored", "Partner Content"],
    &["utm"],
)]
#[case::strip_utm(
    r#"[.[] | .link |= sub("\\?utm.*"; "")]"#,
    &["Linux 7.0", "Sponsored", "Partner Content"],
    &["utm"],
    &[],
)]
#[case::combined(
    r#"[.[] | select(.title | startswith("[Sponsored]") or contains("Partner Content") | not) | .link |= sub("\\?utm.*"; "")]"#,
    &["Linux 7.0"],
    &["Sponsored", "Partner Content", "utm"],
    &[],
)]
fn test_ingest_filter_readme_examples(
    #[case] filter: &str,
    #[case] expect_visible: &[&str],
    #[case] expect_hidden: &[&str],
    #[case] expect_in_export: &[&str],
) {
    let ctx = TestContext::new();

    ctx.run(&["config", "set", "ingest_filter", filter])
        .success();

    let post_date = recent_rss_date(3);
    let sponsored_date = recent_rss_date(2);
    let partner_date = recent_rss_date(1);
    let xml = rss_xml_with_links(
        "Tech News Daily",
        &[
            (
                "Linux 7.0 Released With Exciting New Features",
                &post_date,
                "guid-real-1",
                "https://technews.example.com/linux-7?utm_source=rss",
            ),
            (
                "[Sponsored] Try CloudDB - The Future of Databases",
                &sponsored_date,
                "guid-sponsored-1",
                "https://technews.example.com/clouddb-ad?utm_source=rss",
            ),
            (
                "Partner Content: Why Every Dev Needs AI Insurance",
                &partner_date,
                "guid-partner-1",
                "https://technews.example.com/ai-insurance?utm_source=rss",
            ),
        ],
    );
    ctx.mock_rss_feed("/technews.xml", &xml);
    let url = ctx.server.url("/technews.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    let output = ctx.run(&[".all"]).success().stdout_str();
    for s in expect_visible {
        assert!(
            output.contains(s),
            "expected '{s}' visible, filter={filter}, got:\n{output}"
        );
    }
    for s in expect_hidden {
        assert!(
            !output.contains(s),
            "expected '{s}' hidden, filter={filter}, got:\n{output}"
        );
    }

    let exported = ctx.run(&[".all", "export"]).success().stdout_str();
    for s in expect_in_export {
        assert!(
            exported.contains(s),
            "expected '{s}' in export, filter={filter}, got:\n{exported}"
        );
    }
    // everything in expect_hidden should also be absent from export
    for s in expect_hidden {
        assert!(
            !exported.contains(s),
            "expected '{s}' not in export, filter={filter}, got:\n{exported}"
        );
    }
}

/// When a feed rotates all its posts between syncs (no overlapping GUIDs),
/// the second batch should NOT get initial read marks — only the first pull should.
#[test]
fn test_second_sync_with_rotated_posts_stays_unread() {
    let ctx = TestContext::new();

    // First sync: 3 old posts — initial_read_ids marks 2 as read, keeps 1 unread
    let xml1 = rss_xml_with_guids(
        "Rotating Blog",
        &[
            (
                "Old Post A",
                "Mon, 01 Jan 2024 00:00:00 +0000",
                "guid-old-a",
            ),
            (
                "Old Post B",
                "Tue, 02 Jan 2024 00:00:00 +0000",
                "guid-old-b",
            ),
            (
                "Old Post C",
                "Wed, 03 Jan 2024 00:00:00 +0000",
                "guid-old-c",
            ),
        ],
    );
    let mut mock1 = ctx.server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/rotating.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml1);
    });
    let url = ctx.server.url("/rotating.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Verify: 1 unread (most recent of all-old batch)
    let before = ctx
        .run(&["show", ".unread", "2020-01-01.."])
        .success()
        .stdout_str();
    assert_eq!(
        before.lines().filter(|l| !l.trim().is_empty()).count(),
        1,
        "first sync: expected 1 unread post, got:\n{before}"
    );

    // Replace mock with completely different posts
    mock1.delete();
    let date_x = recent_rss_date(1);
    let date_y = recent_rss_date(2);
    let xml2 = rss_xml_with_guids(
        "Rotating Blog",
        &[
            ("New Post X", &date_x, "guid-new-x"),
            ("New Post Y", &date_y, "guid-new-y"),
        ],
    );
    ctx.server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/rotating.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml2);
    });
    ctx.run(&["sync"]).success();

    // Both new posts should be unread — they arrived after the initial subscribe
    let after = ctx
        .run(&["show", ".unread", "2020-01-01.."])
        .success()
        .stdout_str();
    let unread_lines: Vec<&str> = after.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        unread_lines.iter().any(|l| l.contains("New Post X")),
        "New Post X should be unread, got:\n{after}"
    );
    assert!(
        unread_lines.iter().any(|l| l.contains("New Post Y")),
        "New Post Y should be unread, got:\n{after}"
    );
}

#[test]
fn test_feed_import_opml() {
    let ctx = TestContext::new();

    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <body>
    <outline text="Blog A" xmlUrl="https://example.com/a.xml" />
    <outline text="Blog B" xmlUrl="https://example.com/b.xml" />
    <outline text="Nested" title="Nested">
      <outline text="Blog C" xmlUrl="https://example.com/c.xml" />
    </outline>
  </body>
</opml>"#;

    let opml_path = ctx.dir.path().join("feeds.opml");
    fs::write(&opml_path, opml).unwrap();

    ctx.run(&["feed", "import", opml_path.to_str().unwrap()])
        .success();

    let feeds = ctx.read_feeds();
    let urls: Vec<&str> = feeds.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(
        urls.contains(&"https://example.com/a.xml"),
        "should have feed A, got: {urls:?}"
    );
    assert!(
        urls.contains(&"https://example.com/b.xml"),
        "should have feed B, got: {urls:?}"
    );
    assert!(
        urls.contains(&"https://example.com/c.xml"),
        "should have nested feed C, got: {urls:?}"
    );
    assert_eq!(urls.len(), 3, "should have exactly 3 feeds, got: {urls:?}");
}

/// Write a meta entry directly into the store's meta table.
fn insert_meta(store_dir: &Path, key: &str, value: &str) {
    let meta_dir = store_dir.join("meta");
    fs::create_dir_all(&meta_dir).unwrap();
    let file_path = meta_dir.join("items_.jsonl");
    let digest = Sha256::digest(key.as_bytes());
    let hash: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    let id = &hash[..11]; // same id_length as MetaEntry (EXPECTED_CAPACITY=100 → short hash)
    let entry = serde_json::json!({
        "id": id,
        "key": key,
        "value": value
    });
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .unwrap();
    writeln!(file, "{}", entry).unwrap();
}

#[test]
fn test_schema_version_newer_than_binary_refuses_to_run() {
    let ctx = TestContext::new();

    // Simulate a store written by a much newer version of blogtato
    insert_meta(ctx.dir.path(), "schema_version", "999");

    // Any command that opens the store should fail
    let output = ctx.run(&["show"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("newer") || stderr.contains("update") || stderr.contains("upgrade"),
        "expected error about newer/incompatible version, got: {stderr}"
    );
}

#[test]
fn test_fresh_store_writes_schema_version() {
    let ctx = TestContext::new();
    let xml = rss_xml("Test Blog", &[("Post 1", &recent_rss_date(1))]);
    ctx.mock_rss_feed("/feed.xml", &xml);
    let url = ctx.server.url("/feed.xml");

    ctx.run(&["feed", "add", &url]).success();
    ctx.run(&["sync"]).success();

    // After running commands, the schema version should be written to the meta table
    let meta = read_table(&ctx.dir.path().join("meta"));
    let version_entry = meta
        .iter()
        .find(|e| e.get("key").and_then(|k| k.as_str()) == Some("schema_version"));
    assert!(
        version_entry.is_some(),
        "expected schema_version in meta table, got: {meta:?}"
    );
    assert_eq!(
        version_entry.unwrap()["value"].as_str().unwrap(),
        "1",
        "schema_version should be '1'"
    );
}

#[test]
fn test_feed_export_import_roundtrip() {
    let ctx = TestContext::new();
    let xml_a = rss_xml("Blog Alpha", &[("Post A", &recent_rss_date(1))]);
    let xml_b = rss_xml("Blog Beta", &[("Post B", &recent_rss_date(2))]);
    ctx.mock_rss_feed("/alpha.xml", &xml_a);
    ctx.mock_rss_feed("/beta.xml", &xml_b);

    let url_a = ctx.server.url("/alpha.xml");
    let url_b = ctx.server.url("/beta.xml");
    ctx.run(&["feed", "add", &url_a]).success();
    ctx.run(&["feed", "add", &url_b]).success();
    ctx.run(&["sync"]).success();

    // Export OPML from the first store
    let output = ctx.run(&["feed", "export"]).success();
    let opml = output.stdout_str();

    // Write the exported OPML to a file and import into a fresh store
    let opml_path = ctx.dir.path().join("export.opml");
    fs::write(&opml_path, &opml).unwrap();

    let ctx2 = TestContext::new();
    ctx2.run(&["feed", "import", opml_path.to_str().unwrap()])
        .success();

    // The second store should have the same feeds
    let feeds = ctx2.read_feeds();
    let urls: Vec<&str> = feeds.iter().filter_map(|f| f["url"].as_str()).collect();
    assert!(
        urls.contains(&url_a.as_str()),
        "imported store should have feed A, got: {urls:?}"
    );
    assert!(
        urls.contains(&url_b.as_str()),
        "imported store should have feed B, got: {urls:?}"
    );
    assert_eq!(
        urls.len(),
        2,
        "imported store should have exactly 2 feeds, got: {urls:?}"
    );
}

#[test]
fn test_sync_fetches_posts_from_remotely_added_feed_on_first_sync() {
    let server = MockServer::start();
    let xml = rss_xml(
        "Remote Feed",
        &[("Post from remote feed", &recent_rss_date(1))],
    );
    server.mock(|when, then| {
        when.method(GET).path("/remote-feed.xml");
        then.status(200)
            .header("Content-Type", "application/rss+xml")
            .body(&xml);
    });

    let feed_url = format!("{}/remote-feed.xml", server.base_url());

    let origin_dir = TempDir::new().unwrap();
    git(origin_dir.path(), &["init", "--bare"]);

    // Clone 1 (local)
    let store1 = TempDir::new().unwrap();
    init_git_store(store1.path(), origin_dir.path());

    // Clone 2 (remote device) adds a feed and pushes
    let (other_td, other_dir) = clone_store(origin_dir.path());
    insert_feed(&other_dir, &feed_url);
    git(&other_dir, &["push", "origin", "HEAD"]);
    drop(other_td);

    // Clone 1 syncs ONCE — should discover the remote feed AND fetch its posts
    run_blog(store1.path(), &["sync"]).success();

    // The feed should exist locally
    let feeds = read_table(&store1.path().join("feeds"));
    assert!(
        feeds
            .iter()
            .any(|f| f["url"].as_str() == Some(feed_url.as_str())),
        "local should have the remotely-added feed after one sync"
    );

    // The posts from that feed should also have been fetched
    let posts = read_table(&store1.path().join("posts"));
    assert!(
        posts
            .iter()
            .any(|p| p["title"].as_str() == Some("Post from remote feed")),
        "posts from the remotely-added feed should be fetched on the first sync, \
         but got: {posts:?}"
    );
}

#[test]
fn test_config_set_and_get() {
    let ctx = TestContext::new();
    ctx.run(&["config", "set", "default_query", ".all"])
        .success();
    let output = ctx
        .run(&["config", "get", "default_query"])
        .success()
        .stdout_str();
    assert_eq!(output.trim(), ".all");
}

#[test]
fn test_config_unset() {
    let ctx = TestContext::new();
    ctx.run(&["config", "set", "default_query", ".all"])
        .success();
    ctx.run(&["config", "unset", "default_query"]).success();
    ctx.run(&["config", "get", "default_query"]).failure();
}

#[test]
fn test_custom_default_query_shows_all() {
    let ctx = TestContext::new();

    // Create an old post that the built-in default (90d) would hide
    let posts = r#"{"id":"1","title":"Old Post","date":"2020-01-15T00:00:00Z","feed":"Alice"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    // Built-in default hides it
    ctx.run(&[]).failure();

    // Set a custom default query that shows all posts
    ctx.run(&["config", "set", "default_query", ".all"])
        .success();

    // Now the default show should display the old post
    let output = ctx.run(&[]).success().stdout_str();
    assert!(
        output.contains("Old Post"),
        "custom default_query should override built-in default, got:\n{output}"
    );
}

#[test]
fn test_custom_default_query_groups_by_date() {
    let ctx = TestContext::new();

    let date = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let posts = format!(r#"{{"id":"1","title":"Post A","date":"{date}","feed":"Alice"}}"#);
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), &posts).unwrap();

    // Built-in default groups by week (-W)
    let output = ctx.run(&[]).success().stdout_str();
    assert!(
        output.contains("-W"),
        "built-in default should group by week"
    );

    // Set custom default that groups by date instead of week
    ctx.run(&["config", "set", "default_query", ".unread 90d.. /d"])
        .success();

    // Now show should group by date (YYYY-MM-DD header, no -W)
    let output = ctx.run(&[]).success().stdout_str();
    assert!(
        !output.lines().any(|l| l.contains("-W")),
        "custom default with /d should not group by week, got:\n{output}"
    );
    assert!(
        output.contains(&date[..10]),
        "custom default with /d should group by date, got:\n{output}"
    );
}

#[test]
fn test_filter_by_id_show() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "ID Blog",
        &[
            (
                "Post Alpha",
                &date_a,
                "guid-alpha",
                "https://example.com/alpha",
            ),
            (
                "Post Beta",
                &date_b,
                "guid-beta",
                "https://example.com/beta",
            ),
        ],
    );
    ctx.mock_rss_feed("/id.xml", &xml);
    let url = ctx.server.url("/id.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Get the ID of Post Alpha from export
    let export = ctx.run(&[".all", "export"]).success().stdout_str();
    let alpha_line = export.lines().find(|l| l.contains("Post Alpha")).unwrap();
    let alpha: serde_json::Value = serde_json::from_str(alpha_line).unwrap();
    let alpha_id = alpha["id"].as_str().unwrap();

    // Filter by id: should show only Post Alpha
    let output = ctx
        .run(&[&format!("id:{alpha_id}"), "show"])
        .success()
        .stdout_str();
    assert!(
        output.contains("Post Alpha"),
        "id: filter should show Post Alpha, got:\n{output}"
    );
    assert!(
        !output.contains("Post Beta"),
        "id: filter should NOT show Post Beta, got:\n{output}"
    );
}

#[test]
fn test_filter_by_id_read() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "ID Blog",
        &[
            (
                "Post Alpha",
                &date_a,
                "guid-alpha",
                "https://example.com/alpha",
            ),
            (
                "Post Beta",
                &date_b,
                "guid-beta",
                "https://example.com/beta",
            ),
        ],
    );
    ctx.mock_rss_feed("/id.xml", &xml);
    let url = ctx.server.url("/id.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Get the ID of Post Alpha from export
    let export = ctx.run(&[".all", "export"]).success().stdout_str();
    let alpha_line = export.lines().find(|l| l.contains("Post Alpha")).unwrap();
    let alpha: serde_json::Value = serde_json::from_str(alpha_line).unwrap();
    let alpha_id = alpha["id"].as_str().unwrap();

    // Mark as read using id: filter
    let output = ctx
        .run(&[&format!("id:{alpha_id}"), "read"])
        .success()
        .stdout_str();
    assert!(
        output.contains("https://example.com/alpha"),
        "id: read should print the post URL, got:\n{output}"
    );

    // Verify only Post Alpha is marked as read
    let export2 = ctx.run(&[".all", "export"]).success().stdout_str();
    let alpha_line2 = export2.lines().find(|l| l.contains("Post Alpha")).unwrap();
    let alpha2: serde_json::Value = serde_json::from_str(alpha_line2).unwrap();
    assert!(
        alpha2.get("read_at").is_some(),
        "Post Alpha should be marked as read"
    );

    let beta_line2 = export2.lines().find(|l| l.contains("Post Beta")).unwrap();
    let beta2: serde_json::Value = serde_json::from_str(beta_line2).unwrap();
    assert!(
        beta2.get("read_at").is_none(),
        "Post Beta should still be unread"
    );
}

#[test]
fn test_filter_by_id_export() {
    let ctx = TestContext::new();

    let date_a = recent_rss_date(1);
    let date_b = recent_rss_date(2);
    let xml = rss_xml_with_links(
        "ID Blog",
        &[
            (
                "Post Alpha",
                &date_a,
                "guid-alpha",
                "https://example.com/alpha",
            ),
            (
                "Post Beta",
                &date_b,
                "guid-beta",
                "https://example.com/beta",
            ),
        ],
    );
    ctx.mock_rss_feed("/id.xml", &xml);
    let url = ctx.server.url("/id.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Get the ID of Post Beta from export
    let export = ctx.run(&[".all", "export"]).success().stdout_str();
    let beta_line = export.lines().find(|l| l.contains("Post Beta")).unwrap();
    let beta: serde_json::Value = serde_json::from_str(beta_line).unwrap();
    let beta_id = beta["id"].as_str().unwrap();

    // Export filtered by id: should only return Post Beta
    let output = ctx
        .run(&[&format!("id:{beta_id}"), "export"])
        .success()
        .stdout_str();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 1, "should export exactly 1 post");
    assert!(
        lines[0].contains("Post Beta"),
        "exported post should be Post Beta"
    );
}

#[test]
fn test_filter_by_id_unknown() {
    let ctx = TestContext::new();

    let xml = rss_xml("Test Blog", &[("Post A", &recent_rss_date(1))]);
    ctx.mock_rss_feed("/test.xml", &xml);
    let url = ctx.server.url("/test.xml");
    ctx.write_feeds(&[&url]);
    ctx.run(&["sync"]).success();

    // Unknown ID should fail with a "no matching" or "not found" error, not a parse error
    let output = ctx.run(&["id:deadbeef12345678", "show"]).failure();
    let stderr = output.stderr_str();
    assert!(
        stderr.contains("deadbeef12345678") && !stderr.contains("Failed to parse"),
        "error should mention the unknown ID without being a parse error, got:\n{stderr}"
    );
}

#[test]
fn test_show_includes_summary_footer() {
    let ctx = TestContext::new();

    let posts = r#"{"id":"1","title":"Post A","date":"2024-01-15T00:00:00Z","feed":"feed1"}
{"id":"2","title":"Post B","date":"2024-01-14T00:00:00Z","feed":"feed2"}"#;
    fs::create_dir_all(ctx.dir.path().join("posts")).unwrap();
    fs::write(ctx.dir.path().join("posts").join("items_.jsonl"), posts).unwrap();

    let output = ctx.run(&["show", "2020-01-01.."]).success();
    let stderr = output.stderr_str();

    // Summary footer goes to stderr with post/feed counts and the query
    assert!(
        stderr.contains("Post(s)") && stderr.contains("Feed(s)") && stderr.contains("2020-01-01.."),
        "stderr should contain summary footer with counts and query, got:\n{stderr}"
    );
}
