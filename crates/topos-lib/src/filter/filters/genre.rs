use crate::{data::books::BookId, filter::bible_filter::IsFilter};

pub struct GenreFilter {
    input: String,
}

impl GenreFilter {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

impl IsFilter for GenreFilter {
    fn get_ids(
        &self,
        data: &crate::data::bible_data::BibleData,
    ) -> std::collections::BTreeSet<BookId> {
        data.genres()
            .genre_ids(&self.input)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use crate::filter::{
        bible_filter::Operation,
        filters::{genre::GenreFilter, testament::TestamentFilter},
    };

    macro_rules! mk_test {
        ($fn_name: ident, [$($filter:expr),+ $(,)?], $count:literal) => {
            #[test]
            fn $fn_name() {
                let data = crate::data::bible_data::BibleData::default();
                let mut filter = crate::filter::bible_filter::BibleFilter::new(data);
                $(
                    filter.push($filter);
                )*
                assert_eq!(filter.ids().len(), $count);
            }
        };
    }

    mk_test!(
        pentateuch_only,
        [Operation::Include(GenreFilter::new("pentateuch"))],
        5
    );

    mk_test!(
        no_pentateuch,
        [Operation::Exclude(GenreFilter::new("pentateuch"))],
        61
    );

    mk_test!(
        no_pentateuch_or_gospels,
        [
            Operation::Exclude(GenreFilter::new("pentateuch")),
            Operation::Exclude(GenreFilter::new("gospels")),
        ],
        57
    );

    mk_test!(
        pentateuch_and_gospels,
        [
            Operation::Include(GenreFilter::new("pentateuch")),
            Operation::Include(GenreFilter::new("gospels")),
        ],
        9
    );

    mk_test!(
        nt_without_gospels,
        [
            Operation::Include(TestamentFilter::New),
            Operation::Exclude(GenreFilter::new("gospels")),
        ],
        23
    );

    mk_test!(
        nt_and_gospels,
        [
            Operation::Include(TestamentFilter::New),
            Operation::Include(GenreFilter::new("gospels")),
        ],
        27
    );

    mk_test!(
        nt_without_prophets,
        [
            Operation::Include(TestamentFilter::New),
            Operation::Exclude(GenreFilter::new("prophets")),
        ],
        27
    );
}
