use serde::{Deserialize, Serialize};

use synctato::TableRow;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedSource {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub site_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub is_fetched: bool,
}

impl TableRow for FeedSource {
    fn key(&self) -> String {
        self.url.clone()
    }

    const TABLE_NAME: &'static str = "feeds";
    const SHARD_CHARACTERS: usize = 0;
    // Conservative upper bound; drives hash ID length (11 hex chars) via the
    // birthday-problem formula in synctato. Changing this value would alter ID
    // lengths and orphan rows in existing databases, so leave it stable.
    const EXPECTED_CAPACITY: usize = 50_000;
}
