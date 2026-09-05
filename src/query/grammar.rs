use chrono::{NaiveDate, Utc};
use chumsky::prelude::*;

use super::{GroupKey, QueryDate, ReadFilter};
use crate::utils::date::start_of_day;

pub(super) enum Token {
    Group(GroupKey),
    FeedFilter(String),
    IdFilter(String),
    Range(Option<QueryDate>, Option<QueryDate>),
    Shorthand(String),
    ReadStatus(ReadFilter),
}

fn date_value_core<'a>() -> impl Parser<'a, &'a str, QueryDate, extra::Err<Rich<'a, char>>> {
    let named = choice((just("today").to("today"), just("yesterday").to("yesterday")));

    let digits = text::digits(10).to_slice();

    // Each unit variant carries (duration_unit_for_calc, raw_suffix)
    let relative = digits.then(choice((
        just("months").to(("months", "months")),
        just("m").to(("months", "m")),
        just("w").to(("w", "w")),
        just("d").to(("d", "d")),
    )));

    let absolute = text::digits(10)
        .to_slice()
        .then_ignore(just('-'))
        .then(text::digits(10).to_slice())
        .then_ignore(just('-'))
        .then(text::digits(10).to_slice());

    choice((
        named.map(|name: &str| {
            let resolved = match name {
                "today" => start_of_day(Utc::now().date_naive()),
                "yesterday" => start_of_day((Utc::now() - chrono::Duration::days(1)).date_naive()),
                _ => unreachable!(),
            };
            QueryDate {
                raw: name.to_string(),
                resolved,
            }
        }),
        absolute.try_map(|((year, month), day), span| {
            let s = format!("{year}-{month}-{day}");
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map(|d| QueryDate {
                    raw: s.clone(),
                    resolved: start_of_day(d),
                })
                .map_err(|_| Rich::custom(span, format!("Invalid date: {s}")))
        }),
        relative.try_map(
            |(num_str, (calc_unit, raw_suffix)): (&str, (&str, &str)), span| {
                let n: i64 = num_str.parse().map_err(|_| {
                    Rich::custom(span, format!("Invalid number in duration: {num_str}"))
                })?;
                let duration = match calc_unit {
                    "d" => chrono::Duration::days(n),
                    "w" => chrono::Duration::weeks(n),
                    "months" => chrono::Duration::days(n * 30),
                    _ => unreachable!(),
                };
                Ok(QueryDate {
                    raw: format!("{num_str}{raw_suffix}"),
                    resolved: Utc::now() - duration,
                })
            },
        ),
    ))
    .labelled("date value (e.g. 2024-01-15, 3d, 2w, 1m, today, yesterday)")
}

pub(super) fn date_value_parser<'a>()
-> impl Parser<'a, &'a str, QueryDate, extra::Err<Rich<'a, char>>> {
    date_value_core().then_ignore(end().labelled("end of date value"))
}

pub(super) fn arg_parser<'a>() -> impl Parser<'a, &'a str, Token, extra::Err<Rich<'a, char>>> {
    let group = just('/')
        .ignore_then(one_of("dwf").labelled("grouping: /d (date), /w (week), or /f (feed)"))
        .then_ignore(end().labelled("end of grouping argument"))
        .map(|c| {
            Token::Group(match c {
                'd' => GroupKey::Date,
                'w' => GroupKey::Week,
                'f' => GroupKey::Feed,
                _ => unreachable!(),
            })
        });

    let feed_filter = just('@')
        .ignore_then(any().repeated().at_least(1).collect::<String>())
        .then_ignore(end().labelled("end of feed filter"))
        .map(Token::FeedFilter);

    let range = choice((
        date_value_core()
            .then_ignore(just(".."))
            .then(date_value_core().or_not())
            .then_ignore(end().labelled("end of date range"))
            .map(|(from, to)| Token::Range(Some(from), to)),
        just("..")
            .ignore_then(date_value_core())
            .then_ignore(end().labelled("end of date range"))
            .map(|to| Token::Range(None, Some(to))),
    ));

    let read_status = just('.')
        .ignore_then(choice((
            just("unread").to(ReadFilter::Unread),
            just("read").to(ReadFilter::Read),
            just("all").to(ReadFilter::All),
        )))
        .then_ignore(end().labelled("end of read filter"))
        .map(Token::ReadStatus);

    let id_filter = just("id:")
        .ignore_then(any().repeated().at_least(1).collect::<String>())
        .then_ignore(end().labelled("end of id filter"))
        .map(Token::IdFilter);

    let shorthand = any()
        .filter(|c: &char| c.is_alphanumeric())
        .repeated()
        .at_least(1)
        .collect::<String>()
        .then_ignore(end())
        .map(Token::Shorthand);

    choice((range, group, feed_filter, id_filter, read_status, shorthand)).labelled(
        "argument (3d..1d, /d, /w, /f, @feed, id:<id>, .read, .unread, .all, or shorthand)",
    )
}
