//! Filtering unfiltered results must give exactly what a filtered search finds (the CLI's cache
//! relies on it), and passages must survive the cache's binary format.

use topos_bible::{
    filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    },
    matcher::BibleMatcher,
    segments::Passage,
};

/// Every input in the search cases, as one text
fn corpus() -> String {
    include_str!("cases/search.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split(" => ").next())
        .map(|input| input.trim_start_matches('?').replace("\\n", "\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn filters() -> Vec<(&'static str, BibleFilter)> {
    let mut cases = vec![("none", BibleFilter::default())];
    let mut add = |name, build: &dyn Fn(&mut BibleFilter)| {
        let mut filter = BibleFilter::default();
        build(&mut filter);
        cases.push((name, filter));
    };
    add("nt", &|f| f.include(TestamentFilter::New).unwrap());
    add("ot + pauline", &|f| {
        f.include(TestamentFilter::Old).unwrap();
        f.include(GenreFilter::new("Pauline Epistles")).unwrap();
    });
    add("gospels - john", &|f| {
        f.include(GenreFilter::new("Gospels")).unwrap();
        f.exclude(BookFilter::new("John")).unwrap();
    });
    add("inside john 3", &|f| f.filter_inside("John 3").unwrap());
    add("overlaps rom 8, gen 1", &|f| {
        f.filter_any_overlap("Romans 8").unwrap();
        f.filter_any_overlap("Genesis 1").unwrap();
    });
    add(
        "explicit john 3:16, exact rom 8:28, excluding ps 23",
        &|f| {
            f.filter_explicit_overlap("John 3:16").unwrap();
            f.filter_exact_overlap("Romans 8:28").unwrap();
            f.filter_exclude_overlap("Psalm 23").unwrap();
        },
    );
    add("nt outside john", &|f| {
        f.include(TestamentFilter::New).unwrap();
        f.filter_exclude_overlap("John 1-21").unwrap();
    });
    cases
}

#[test]
fn filtering_unfiltered_results_matches_a_filtered_search() {
    let text = corpus();
    for (name, filter) in filters() {
        let matcher = filter.create_matcher();
        let direct: Vec<_> = matcher
            .search(&text)
            .into_iter()
            .map(|m| (m.location.bytes.start..m.location.bytes.end, m.psg))
            .collect();
        let later: Vec<_> = matcher
            .without_filters()
            .search(&text)
            .into_iter()
            .filter(|m| matcher.keeps(&m.psg))
            .map(|m| (m.location.bytes.start..m.location.bytes.end, m.psg))
            .collect();
        assert_eq!(direct, later, "{name}");
        // A kept match is always in one of the possible books
        if let Some(books) = matcher.possible_books() {
            assert!(
                direct.iter().all(|(_, psg)| books.contains(&psg.book)),
                "{name}"
            );
        }
    }
}

#[test]
fn passages_round_trip_through_json_and_binary() {
    let matcher = BibleMatcher::default();
    let passages: Vec<Passage> = matcher
        .search(&corpus())
        .into_iter()
        .map(|m| m.psg)
        .collect();
    assert!(passages.len() > 50);
    let json = serde_json::to_string(&passages).unwrap();
    // JSON keeps the untagged form: segments are just their fields
    assert!(!json.contains("ChapterVerse"), "{}", &json[..200]);
    assert_eq!(
        serde_json::from_str::<Vec<Passage>>(&json).unwrap(),
        passages
    );
    let bytes = postcard::to_allocvec(&passages).unwrap();
    assert_eq!(
        postcard::from_bytes::<Vec<Passage>>(&bytes).unwrap(),
        passages
    );
}
