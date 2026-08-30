mod commands;
mod data;
mod display;
mod feed;
mod query;
mod shorthand;
mod utils;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use shorthand::RESERVED_COMMANDS;

/// A simple RSS/Atom feed reader
#[derive(Parser)]
#[command(after_help = QUERY_HELP)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

const QUERY_HELP: &str = "\
QUERY LANGUAGE:
  Grouping (up to 2):
    /d          Group by date
    /w          Group by week
    /f          Group by feed

  Filtering:
    @shorthand  Show only posts from a specific feed
    .read       Show only read posts
    .unread     Show only unread posts
    .all        Show all posts (override default filter)

  Date range:
    <date>..<date>  Date range (e.g. 3m..1m)
    <date>..        Open-ended range (from date onward)
    ..<date>        Open-ended range (up to date)

  Date values:
    2024-01-15  Absolute date (YYYY-MM-DD)
    3d          Relative: 3 days ago
    2w          Relative: 2 weeks ago
    1m          Relative: 1 month ago
    today       Start of today
    yesterday   Start of yesterday

EXAMPLES:
  blog /d                     Group by date
  blog /f /d                  Group by feed, then date
  blog @myblog                Show only posts from @myblog
  blog 1w..                   Posts from the last week
  blog 3m..1m                 Posts from 1-3 months ago
  blog /d 2w..1w              Posts from 1-2 weeks ago, grouped by date
  blog a open                 Open post with shorthand 'a'
  blog a read                 Print URL of post 'a'
  blog a unread               Mark post 'a' as unread
  blog .unread                Show only unread posts
  blog @myblog .unread        Show unread posts from @myblog
  blog .all                   Show all posts (bypass defaults)
  blog .all export             Export all posts as JSONL
  blog @myblog export          Export posts from @myblog as JSONL";

#[derive(Subcommand)]
enum Command {
    /// Display items from posts.jsonl
    #[command(after_help = QUERY_HELP)]
    Show {
        /// Query arguments (see below)
        args: Vec<String>,
    },
    /// Open a post in the default browser
    Open,
    /// Print the URL of a post to stdout
    Read,
    /// Manage feed subscriptions
    Feed {
        #[command(subcommand)]
        command: FeedCommand,
    },
    /// Fetch feeds and sync with remote
    Sync {
        /// Repeat to sync only selected feeds by @shorthand
        #[arg(long = "feed", value_name = "SHORTHAND")]
        feeds: Vec<String>,
        /// Do not interact with the remote repository
        #[arg(long)]
        no_remote: bool,
    },
    /// Mark a post as unread
    Unread,
    /// Export matching posts as JSONL
    #[command(after_help = QUERY_HELP)]
    Export {
        /// Query arguments (see below)
        args: Vec<String>,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Run git commands in the store directory
    Git {
        /// Arguments to pass to git
        args: Vec<String>,
    },
    /// Clone an existing feed database from a git remote
    Clone {
        /// Git-clonable URL
        url: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Set a config value
    Set {
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
    /// Get a config value
    Get {
        /// Config key
        key: String,
    },
    /// Remove a config value
    Unset {
        /// Config key
        key: String,
    },
}

#[derive(Subcommand)]
enum FeedCommand {
    /// Subscribe to a feed by URL
    Add {
        /// The feed URL to subscribe to
        urls: Vec<String>,
    },
    /// Unsubscribe from a feed by URL or @shorthand
    Rm {
        /// The feed URL or @shorthand to unsubscribe from
        urls: Vec<String>,
    },
    /// List subscribed feeds
    Ls,
    /// Import feeds from an OPML file
    Import {
        /// Path to the OPML file
        path: std::path::PathBuf,
    },
    /// Export feeds as OPML to stdout
    Export,
}

fn split_at_command(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let cmd_pos = args[1..]
        .iter()
        .position(|a| RESERVED_COMMANDS.contains(&a.as_str()));
    let split = cmd_pos.map(|p| p + 1).unwrap_or(args.len());
    let (flags, filter): (Vec<_>, Vec<_>) = args[1..split]
        .iter()
        .cloned()
        .partition(|a| a.starts_with('-'));
    let mut cmd_args = vec![args[0].clone()];
    cmd_args.extend(flags);
    cmd_args.extend_from_slice(&args[split..]);
    (filter, cmd_args)
}

fn store_dir() -> anyhow::Result<PathBuf> {
    if let Ok(val) = std::env::var("RSS_STORE") {
        return Ok(PathBuf::from(val));
    }
    // Only one store ("default") is supported for now. The stores/ nesting
    // keeps the door open for multiple named stores in the future.
    dirs::data_dir()
        .map(|d| d.join("blogtato").join("stores").join("default"))
        .ok_or_else(|| anyhow::anyhow!("could not determine data directory; set RSS_STORE"))
}

fn parse_query_or_default(
    args: &[String],
    store: &data::BlogData,
) -> anyhow::Result<(query::Query, String)> {
    let query = if args.is_empty() {
        let default = data::get_config_value(store, "default_query");
        let text = default.as_deref().unwrap_or(query::DEFAULT_QUERY);
        query::parse_query_str(text)?
    } else {
        query::parse_query(args)?
    };
    let text = query.to_string();
    Ok((query, text))
}

fn reject_filter(filter: &[String], command: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        filter.is_empty(),
        "{command} command does not accept a filter"
    );
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let (filter, cmd_args) = split_at_command(std::env::args().collect());
    let args = Args::parse_from(cmd_args);
    let store_dir = store_dir()?;

    if let Some(Command::Clone { ref url }) = args.command {
        return commands::clone::cmd_clone(&store_dir, url);
    }

    let mut store = data::BlogData::open(&store_dir)?;
    data::check_schema_version(&mut store)?;

    match args.command {
        // Commands that accept a query/filter
        Some(Command::Show { ref args }) => {
            let all_args: Vec<String> = filter.into_iter().chain(args.iter().cloned()).collect();
            let (q, query_text) = parse_query_or_default(&all_args, &store)?;
            commands::show::cmd_show(&store, &q, &query_text)?;
        }
        Some(Command::Export { ref args }) => {
            let all_args: Vec<String> = filter.into_iter().chain(args.iter().cloned()).collect();
            let (q, _) = parse_query_or_default(&all_args, &store)?;
            commands::export::cmd_export(&store, &q)?;
        }
        Some(Command::Open) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_open(&mut store, &q)?;
        }
        Some(Command::Read) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_read(&mut store, &q)?;
        }
        Some(Command::Unread) => {
            let q = query::parse_query(&filter)?;
            commands::open::cmd_unread(&mut store, &q)?;
        }
        None => {
            let (q, query_text) = parse_query_or_default(&filter, &store)?;
            commands::show::cmd_show(&store, &q, &query_text)?;
        }

        // Commands that reject filters
        Some(Command::Feed {
            command: FeedCommand::Add { ref urls },
        }) => {
            reject_filter(&filter, "feed")?;
            for url in urls.iter().filter(|url| !url.is_empty()) {
                let resolved = commands::add::resolve_feed_url(url)?;
                if resolved != *url {
                    eprintln!("Discovered feed: {resolved}");
                }
                store.transact(&format!("add feed: {resolved}"), |tx| {
                    commands::add::cmd_add(tx, &resolved)
                })?;
                eprintln!("Added {resolved}");
            }
            eprintln!("Run `blog sync` to fetch posts.");
        }
        Some(Command::Feed {
            command: FeedCommand::Rm { ref urls },
        }) => {
            reject_filter(&filter, "feed")?;
            for url in urls.iter().filter(|url| !url.is_empty()) {
                store.transact(&format!("remove {url}"), |tx| {
                    commands::remove::cmd_remove(tx, url)
                })?;
            }
        }
        Some(Command::Feed {
            command: FeedCommand::Ls,
        }) => {
            reject_filter(&filter, "feed")?;
            commands::feed_ls::cmd_feed_ls(&store)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Export,
        }) => {
            reject_filter(&filter, "feed")?;
            commands::feed_export::cmd_feed_export(&store)?;
        }
        Some(Command::Feed {
            command: FeedCommand::Import { ref path },
        }) => {
            reject_filter(&filter, "feed")?;
            commands::import::cmd_import(&mut store, path)?;
        }

        Some(Command::Sync {
            ref feeds,
            no_remote,
        }) => {
            reject_filter(&filter, "sync")?;
            commands::sync::cmd_sync(&mut store, feeds, no_remote)?;
        }
        Some(Command::Git { ref args }) => {
            reject_filter(&filter, "git")?;
            store.git_passthrough(args)?;
        }
        Some(Command::Config {
            command: ConfigCommand::Set { ref key, ref value },
        }) => {
            reject_filter(&filter, "config")?;
            commands::config::cmd_config_set(&mut store, key, value)?;
        }
        Some(Command::Config {
            command: ConfigCommand::Get { ref key },
        }) => {
            reject_filter(&filter, "config")?;
            commands::config::cmd_config_get(&store, key)?;
        }
        Some(Command::Config {
            command: ConfigCommand::Unset { ref key },
        }) => {
            reject_filter(&filter, "config")?;
            commands::config::cmd_config_unset(&mut store, key)?;
        }
        Some(Command::Clone { .. }) => unreachable!(),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(input: &[&str]) -> Vec<String> {
        input.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_split_at_command_filter_before_command() {
        let (filter, cmd) = split_at_command(args(&["blog", "a", "open"]));
        assert_eq!(filter, args(&["a"]));
        assert_eq!(cmd, args(&["blog", "open"]));
    }

    #[test]
    fn test_split_at_command_no_filter() {
        let (filter, cmd) = split_at_command(args(&["blog", "sync"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "sync"]));
    }

    #[test]
    fn test_split_at_command_sync_with_feed_flags() {
        let (filter, cmd) = split_at_command(args(&["blog", "sync", "--feed", "@df"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "sync", "--feed", "@df"]));
    }

    #[test]
    fn test_split_at_command_no_command() {
        let (filter, cmd) = split_at_command(args(&["blog", "/d", "@hn"]));
        assert_eq!(filter, args(&["/d", "@hn"]));
        assert_eq!(cmd, args(&["blog"]));
    }

    #[test]
    fn test_split_at_command_help_flag() {
        let (filter, cmd) = split_at_command(args(&["blog", "--help"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "--help"]));
    }

    #[test]
    fn test_split_at_command_feed_subcommand() {
        let (filter, cmd) = split_at_command(args(&["blog", "feed", "add", "url"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "feed", "add", "url"]));
    }

    #[test]
    fn test_split_at_command_show_with_args() {
        let (filter, cmd) = split_at_command(args(&["blog", "show", "/d"]));
        assert_eq!(filter, Vec::<String>::new());
        assert_eq!(cmd, args(&["blog", "show", "/d"]));
    }

    #[test]
    fn test_split_at_command_filter_with_show() {
        let (filter, cmd) = split_at_command(args(&["blog", "/d", "show"]));
        assert_eq!(filter, args(&["/d"]));
        assert_eq!(cmd, args(&["blog", "show"]));
    }

    #[test]
    fn test_reserved_commands_match_clap_subcommands() {
        use clap::CommandFactory;
        let clap_names: std::collections::HashSet<_> = Args::command()
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();
        let reserved: std::collections::HashSet<_> =
            RESERVED_COMMANDS.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            clap_names, reserved,
            "RESERVED_COMMANDS must match clap subcommands"
        );
    }

    #[test]
    fn test_parse_sync_without_feed_selectors() {
        let args = Args::parse_from(args(&["blog", "sync"]));
        let Some(Command::Sync { feeds, no_remote }) = args.command else {
            panic!("expected sync command");
        };
        assert!(feeds.is_empty());
        assert_eq!(no_remote, false);
    }

    #[test]
    fn test_parse_sync_with_one_feed_selector() {
        let args = Args::parse_from(args(&["blog", "sync", "--feed", "@df"]));
        let Some(Command::Sync { feeds, no_remote }) = args.command else {
            panic!("expected sync command");
        };
        assert_eq!(feeds, vec!["@df"]);
        assert_eq!(no_remote, false);
    }

    #[test]
    fn test_parse_sync_with_multiple_feed_selectors() {
        let args = Args::parse_from(args(&["blog", "sync", "--feed", "@df", "--feed", "@dg"]));
        let Some(Command::Sync { feeds, no_remote }) = args.command else {
            panic!("expected sync command");
        };
        assert_eq!(feeds, vec!["@df", "@dg"]);
        assert_eq!(no_remote, false);
    }

    #[test]
    fn test_parse_sync_with_no_remote() {
        let args = Args::parse_from(args(&["blog", "sync", "--no-remote"]));
        let Some(Command::Sync { feeds, no_remote }) = args.command else {
            panic!("expected sync command");
        };
        assert!(feeds.is_empty());
        assert_eq!(no_remote, true);
    }

    #[test]
    fn test_parse_sync_with_feed_selectors_and_no_remote() {
        let args = Args::parse_from(args(&[
            "blog",
            "sync",
            "--feed",
            "@df",
            "--no-remote",
            "--feed",
            "@dg",
        ]));
        let Some(Command::Sync { feeds, no_remote }) = args.command else {
            panic!("expected sync command");
        };
        assert_eq!(feeds, vec!["@df", "@dg"]);
        assert_eq!(no_remote, true);
    }
}
