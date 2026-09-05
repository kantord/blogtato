use std::fs;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;

use crate::data::BlogData;

use super::add::cmd_add;

pub(crate) fn cmd_import(store: &mut BlogData, path: &Path) -> anyhow::Result<()> {
    let content = fs::read_to_string(path)?;
    let urls = parse_opml_urls(&content);

    if urls.is_empty() {
        anyhow::bail!("no feeds found in {}", path.display());
    }

    if let Some(url) = urls.iter().find(|url| url.starts_with("stdin:")) {
        anyhow::bail!(
            "{url} is an ingest-only feed and cannot be imported; \
             pipe its RSS/Atom into `blog feed ingest` instead"
        );
    }

    store.transact(&format!("import {} feeds from OPML", urls.len()), |tx| {
        for url in &urls {
            cmd_add(tx, url)?;
        }
        Ok(())
    })?;

    eprintln!("Imported {} feeds.", urls.len());
    eprintln!("Run `blog sync` to fetch posts.");
    Ok(())
}

fn parse_opml_urls(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut urls = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e) | Event::Start(ref e)) if e.name().as_ref() == "outline" => {
                if let Some(url) = extract_xml_url(e) {
                    urls.push(url);
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }

    urls
}

fn extract_xml_url(element: &quick_xml::events::BytesStart) -> Option<String> {
    let attr = element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == "xmlUrl")?;
    let value = attr.normalized_value(XmlVersion::Implicit1_0).ok()?;
    let url = value.trim().to_string();
    (!url.is_empty()).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opml_simple() {
        let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <body>
    <outline text="A" xmlUrl="https://example.com/a.xml" />
    <outline text="B" xmlUrl="https://example.com/b.xml" />
  </body>
</opml>"#;
        let urls = parse_opml_urls(opml);
        assert_eq!(
            urls,
            vec!["https://example.com/a.xml", "https://example.com/b.xml",]
        );
    }

    #[test]
    fn test_parse_opml_nested() {
        let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <body>
    <outline text="Category">
      <outline text="C" xmlUrl="https://example.com/c.xml" />
    </outline>
  </body>
</opml>"#;
        let urls = parse_opml_urls(opml);
        assert_eq!(urls, vec!["https://example.com/c.xml"]);
    }

    #[test]
    fn test_parse_opml_empty() {
        let opml = r#"<?xml version="1.0"?><opml><body></body></opml>"#;
        assert!(parse_opml_urls(opml).is_empty());
    }

    #[test]
    fn test_parse_opml_xml_entities() {
        let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="1.0">
  <body>
    <outline type="rss" text="News" xmlUrl="https://news.google.com/rss/search?hl=es-419&amp;gl=US&amp;ceid=US:es-419&amp;q=Test" />
  </body>
</opml>"#;
        let urls = parse_opml_urls(opml);
        assert_eq!(
            urls,
            vec!["https://news.google.com/rss/search?hl=es-419&gl=US&ceid=US:es-419&q=Test",]
        );
    }
}
