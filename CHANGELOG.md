# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.25](https://github.com/kantord/blogtato/compare/v0.1.24...v0.1.25) - 2026-05-12

### Fixed

- *(deps)* update rust crate quick-xml to v0.40.0
- *(deps)* update rust crate rss to v2.0.13
- *(deps)* update rust crate open to v5.3.5
- *(deps)* update rust crate atom_syndication to v0.12.8
- *(deps)* update rust crate quick-xml to v0.39.4
- *(deps)* update rust crate chumsky to v0.13.0
- *(deps)* update rust crate quick-xml to v0.39.3

### Other

- merge
- *(deps)* update release-plz/action digest to 064f4d1
- *(deps)* update rust crate assert_cmd to v2.2.2

## [0.1.24](https://github.com/kantord/blogtato/compare/v0.1.23...v0.1.24) - 2026-05-02

### Added

- serialize query back to token syntax via Display impl
- summary footer with query echo ([#172](https://github.com/kantord/blogtato/pull/172))

### Fixed

- *(deps)* update rust crate open to v5.3.4
- use sort_by instead of sort_by_key for date ordering
- *(deps)* update rust crate clap to v4.6.1 ([#164](https://github.com/kantord/blogtato/pull/164))
- *(deps)* update rust crate rayon to v1.12.0 ([#162](https://github.com/kantord/blogtato/pull/162))

### Other

- *(deps)* update rust crate assert_cmd to v2.2.1
- *(deps)* update rust crate libc to v0.2.186
- make clippy happy
- fix failing ci and prek-related issues
- fix clippy error ([#170](https://github.com/kantord/blogtato/pull/170))
- *(deps)* update rust crate libc to v0.2.185 ([#161](https://github.com/kantord/blogtato/pull/161))

## [0.1.23](https://github.com/kantord/blogtato/compare/v0.1.22...v0.1.23) - 2026-04-06

### Added

- add sync only selected feeds

### Other

- add Custom store location description with RSS_STORE env ([#157](https://github.com/kantord/blogtato/pull/157))

## [0.1.22](https://github.com/kantord/blogtato/compare/v0.1.21...v0.1.22) - 2026-04-02

### Added

- allow filtering by db id ([#156](https://github.com/kantord/blogtato/pull/156))
- include db id in export results ([#153](https://github.com/kantord/blogtato/pull/153))

### Other

- *(deps)* update rust crate libc to v0.2.184 ([#155](https://github.com/kantord/blogtato/pull/155))

## [0.1.21](https://github.com/kantord/blogtato/compare/v0.1.20...v0.1.21) - 2026-03-29

### Added

- allow removing/adding multiple feeds at once ([#99](https://github.com/kantord/blogtato/pull/99)) ([#148](https://github.com/kantord/blogtato/pull/148))

### Fixed

- *(deps)* update rust crate synctato to v0.1.12 ([#151](https://github.com/kantord/blogtato/pull/151))
- *(deps)* update rust crate terminal_size to v0.4.4 ([#144](https://github.com/kantord/blogtato/pull/144))

### Other

- *(deps)* update rust crate sha2 to v0.11.0 ([#146](https://github.com/kantord/blogtato/pull/146))
- *(deps)* update dtolnay/rust-toolchain digest to 29eef33 ([#150](https://github.com/kantord/blogtato/pull/150))
- *(deps)* pin dtolnay/rust-toolchain action to 631a55b ([#149](https://github.com/kantord/blogtato/pull/149))

## [0.1.20](https://github.com/kantord/blogtato/compare/v0.1.19...v0.1.20) - 2026-03-21

### Added

- improve feed finder logic ([#143](https://github.com/kantord/blogtato/pull/143))

### Fixed

- *(deps)* update rust crate ureq to v3.3.0 ([#142](https://github.com/kantord/blogtato/pull/142))
- *(deps)* update rust crate ureq to v3.2.1 ([#140](https://github.com/kantord/blogtato/pull/140))

## [0.1.19](https://github.com/kantord/blogtato/compare/v0.1.18...v0.1.19) - 2026-03-16

### Added

- allow configuring default query ([#111](https://github.com/kantord/blogtato/pull/111))
- allow ingest filtering

### Fixed

- *(deps)* update rust crate synctato to v0.1.11 ([#132](https://github.com/kantord/blogtato/pull/132))
- *(deps)* update rust crate synctato to v0.1.10 ([#130](https://github.com/kantord/blogtato/pull/130))
- *(deps)* pin rust crate url-normalize to =0.1.1 ([#128](https://github.com/kantord/blogtato/pull/128))
- normalize feed urls ([#127](https://github.com/kantord/blogtato/pull/127))
- correctly validate date ranges ([#126](https://github.com/kantord/blogtato/pull/126))
- do not leave dangling read marks ([#125](https://github.com/kantord/blogtato/pull/125))
- sanitize control characters from input strings ([#124](https://github.com/kantord/blogtato/pull/124))
- prevent panic in feed discovery on non-ASCII HTML ([#123](https://github.com/kantord/blogtato/pull/123))
- allow stable feed shorthand matching ([#121](https://github.com/kantord/blogtato/pull/121))

### Other

- add jq based ingest filtering ([#138](https://github.com/kantord/blogtato/pull/138))
- move content filtering to ingest layer ([#137](https://github.com/kantord/blogtato/pull/137))
- unifiy config system ([#136](https://github.com/kantord/blogtato/pull/136))
- enable testing on windows ([#133](https://github.com/kantord/blogtato/pull/133))

## [0.1.18](https://github.com/kantord/blogtato/compare/v0.1.17...v0.1.18) - 2026-03-14

### Fixed

- discover feeds on deep URLs by trying root paths first ([#119](https://github.com/kantord/blogtato/pull/119))

## [0.1.17](https://github.com/kantord/blogtato/compare/v0.1.16...v0.1.17) - 2026-03-13

### Fixed

- use proper headers for version check request ([#117](https://github.com/kantord/blogtato/pull/117))

## [0.1.16](https://github.com/kantord/blogtato/compare/v0.1.15...v0.1.16) - 2026-03-13

### Fixed

- *(deps)* pin rust crate ureq to =3.2.0 ([#115](https://github.com/kantord/blogtato/pull/115))
- *(deps)* update rust crate quick-xml to v0.39.2 ([#94](https://github.com/kantord/blogtato/pull/94))

### Other

- use hand-rolled feed finder logic ([#116](https://github.com/kantord/blogtato/pull/116))
- replace reqwest with ureq ([#113](https://github.com/kantord/blogtato/pull/113))

## [0.1.15](https://github.com/kantord/blogtato/compare/v0.1.14...v0.1.15) - 2026-03-13

### Added

- warn user when using outdated version ([#112](https://github.com/kantord/blogtato/pull/112))

### Fixed

- *(deps)* update rust crate clap to v4.6.0 ([#109](https://github.com/kantord/blogtato/pull/109))

### Other

- simplify default query logic ([#110](https://github.com/kantord/blogtato/pull/110))
- *(deps)* update swatinem/rust-cache digest to e18b497 ([#107](https://github.com/kantord/blogtato/pull/107))
- add comparison with alternatives ([#106](https://github.com/kantord/blogtato/pull/106))
- move build_feed_labels to a more appropriate place ([#105](https://github.com/kantord/blogtato/pull/105))

## [0.1.14](https://github.com/kantord/blogtato/compare/v0.1.13...v0.1.14) - 2026-03-09

### Fixed

- fetch posts also from new feeds coming from remote ([#100](https://github.com/kantord/blogtato/pull/100))

## [0.1.13](https://github.com/kantord/blogtato/compare/v0.1.12...v0.1.13) - 2026-03-08

### Added

- allow exporting feed list to opml ([#95](https://github.com/kantord/blogtato/pull/95))

## [0.1.12](https://github.com/kantord/blogtato/compare/v0.1.11...v0.1.12) - 2026-03-08

### Added

- prevent data loss due to old versions after future migrations ([#93](https://github.com/kantord/blogtato/pull/93))
- allow importing opml files ([#92](https://github.com/kantord/blogtato/pull/92))

### Other

- fix typo: s/you likely has/you likely have/ ([#90](https://github.com/kantord/blogtato/pull/90))
- fix markdown formatting ([#88](https://github.com/kantord/blogtato/pull/88))
- mention subscribing to blogtato releases ([#86](https://github.com/kantord/blogtato/pull/86))

## [0.1.11](https://github.com/kantord/blogtato/compare/v0.1.10...v0.1.11) - 2026-03-08

### Added

- mark old posts as read when subscribing to feed ([#83](https://github.com/kantord/blogtato/pull/83))

### Other

- *(deps)* update rust crate libc to v0.2.183 ([#85](https://github.com/kantord/blogtato/pull/85))

## [0.1.10](https://github.com/kantord/blogtato/compare/v0.1.9...v0.1.10) - 2026-03-07

### Added

- easily merge remote and local db ([#81](https://github.com/kantord/blogtato/pull/81))
- remove redundant date filter

### Fixed

- fix failing test ([#82](https://github.com/kantord/blogtato/pull/82))

## [0.1.9](https://github.com/kantord/blogtato/compare/blogtato-v0.1.8...blogtato-v0.1.9) - 2026-03-07

### Added

- *(synctato)* add Table::delete_where() ([#70](https://github.com/kantord/blogtato/pull/70))
- *(synctato)* add Table::iter() for zero-clone iteration

### Other

- improve blog sync performance ([#76](https://github.com/kantord/blogtato/pull/76))
- *(deps)* update release-plz/action digest to 1528104 ([#74](https://github.com/kantord/blogtato/pull/74))
- add performance measuring scripts ([#75](https://github.com/kantord/blogtato/pull/75))
- avoid per-item SHA-256 hashing in read status filter ([#73](https://github.com/kantord/blogtato/pull/73))
- avoid allocations in GroupKey sort comparator
- deduplicate shorthand lookup in FeedIndex
- extarct MAX_FEED_CANDIDATES
- deduplicate spinner creation
- extract BLOG_NAME_BUDGET_PERCENT
- explain shorthand logic and design a bit
- replace ensure_no_query with reject_filter to avoid unnecessary parsing
- move query execution logic from data/index to query/resolve
- replace tautological assertion
- small fixes
- extract start_of_day helper
- use partition in split_at_command
- extract Query::or_default_view
- replace parallel vectors in FeedIndex with Vec<FeedEntry>
- reuse Style between group.rs and item.rs
- break down format_item into smaller bits
- accept &RenderCtx in render_grouped instead of 7 positional args

## [0.1.8](https://github.com/kantord/blogtato/compare/blogtato-v0.1.7...blogtato-v0.1.8) - 2026-03-06

### Added

- add export command

### Other

- split display module ([#61](https://github.com/kantord/blogtato/pull/61))
- split up query.rs ([#60](https://github.com/kantord/blogtato/pull/60))
- create a utils folder ([#59](https://github.com/kantord/blogtato/pull/59))
- remove useless test module ([#58](https://github.com/kantord/blogtato/pull/58))
- add data folder ([#57](https://github.com/kantord/blogtato/pull/57))
- move shorthand logic to separate file ([#56](https://github.com/kantord/blogtato/pull/56))
- create synctato crate ([#55](https://github.com/kantord/blogtato/pull/55))
- create tables module ([#54](https://github.com/kantord/blogtato/pull/54))

## [0.1.7](https://github.com/kantord/blogtato/compare/v0.1.6...v0.1.7) - 2026-03-05

### Added

- add default query ([#50](https://github.com/kantord/blogtato/pull/50))

## [0.1.6](https://github.com/kantord/blogtato/compare/v0.1.5...v0.1.6) - 2026-03-05

### Added

- allow filtering for read and unread posts ([#49](https://github.com/kantord/blogtato/pull/49))
- make syntax more similar to taskwarrior ([#47](https://github.com/kantord/blogtato/pull/47))
- allow marking post unread ([#46](https://github.com/kantord/blogtato/pull/46))

### Fixed

- avoid race condition when running multiple sync at the same time ([#43](https://github.com/kantord/blogtato/pull/43))

### Other

- small refactors ([#48](https://github.com/kantord/blogtato/pull/48))
- reuse RenderCtx ([#45](https://github.com/kantord/blogtato/pull/45))

## [0.1.5](https://github.com/kantord/blogtato/compare/v0.1.4...v0.1.5) - 2026-03-03

### Added

- track read/unread status for articles ([#41](https://github.com/kantord/blogtato/pull/41))

### Fixed

- avoid making empty commits ([#40](https://github.com/kantord/blogtato/pull/40))
- support slightly malformed timezones ([#38](https://github.com/kantord/blogtato/pull/38))

### Other

- move git logic into db

## [0.1.4](https://github.com/kantord/blogtato/compare/v0.1.3...v0.1.4) - 2026-03-03

### Fixed

- git operations sometimes hang ([#35](https://github.com/kantord/blogtato/pull/35))

### Other

- do not rely on comments to structure code ([#37](https://github.com/kantord/blogtato/pull/37))

## [0.1.3](https://github.com/kantord/blogtato/compare/v0.1.2...v0.1.3) - 2026-03-02

### Added

- allow filtering by date ([#30](https://github.com/kantord/blogtato/pull/30))
- introduce free-combination grouping syntax ([#28](https://github.com/kantord/blogtato/pull/28))

### Other

- explain git based sync decision ([#34](https://github.com/kantord/blogtato/pull/34))
- update documentation to reflect new filtering logic ([#33](https://github.com/kantord/blogtato/pull/33))
- *(deps)* update rust crate rstest to 0.26 ([#32](https://github.com/kantord/blogtato/pull/32))
- make test code less verbose ([#31](https://github.com/kantord/blogtato/pull/31))

## [0.1.2](https://github.com/kantord/blogtato/compare/v0.1.1...v0.1.2) - 2026-03-01

### Added

- truncate columns to fit terminal width ([#25](https://github.com/kantord/blogtato/pull/25))
- pad shorthands in output ([#23](https://github.com/kantord/blogtato/pull/23))

### Fixed

- *(deps)* update rust crate terminal_size to v0.4.3 ([#26](https://github.com/kantord/blogtato/pull/26))
- support unicode truncation properly ([#27](https://github.com/kantord/blogtato/pull/27))

## [0.1.1](https://github.com/kantord/blogtato/compare/v0.1.0...v0.1.1) - 2026-03-01

### Added

- add some color to output ([#22](https://github.com/kantord/blogtato/pull/22))

### Fixed

- defensively use a more flexible db location ([#21](https://github.com/kantord/blogtato/pull/21))

### Other

- *(deps)* update actions/checkout action to v6 ([#19](https://github.com/kantord/blogtato/pull/19))
- *(deps)* pin dependencies ([#18](https://github.com/kantord/blogtato/pull/18))
