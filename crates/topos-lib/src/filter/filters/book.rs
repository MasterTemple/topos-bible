use std::collections::BTreeSet;

use crate::{
    data::books::BookId,
    error::{ToposError, ToposResult},
    filter::bible_filter::IsFilter,
};

pub struct BookFilter {
    input: String,
}

impl BookFilter {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
        }
    }
}

impl IsFilter for BookFilter {
    fn get_ids(&self, data: &crate::data::bible_data::BibleData) -> ToposResult<BTreeSet<BookId>> {
        let book = data
            .books()
            .search(&self.input)
            .ok_or_else(|| ToposError::UnknownBook(self.input.clone()))?;
        Ok(BTreeSet::from([book]))
    }
}

#[cfg(test)]
mod tests {
    use crate::filter::{
        bible_filter::Operation,
        filters::{book::BookFilter, genre::GenreFilter},
    };

    macro_rules! mk_test {
        ($fn_name: ident, [$($filter:expr),+ $(,)?], $count:literal) => {
            #[test]
            fn $fn_name() {
                let data = crate::data::bible_data::BibleData::default();
                let mut filter = crate::filter::bible_filter::BibleFilter::new(data);
                $(
                    filter.push($filter).unwrap();
                )*
                assert_eq!(filter.ids().len(), $count);
            }
        };
    }

    mk_test!(
        pentateuch_without_genesis,
        [
            Operation::Include(GenreFilter::new("pentateuch")),
            Operation::Exclude(BookFilter::new("genesis")),
        ],
        4
    );

    mk_test!(
        pentateuch_and_revelation,
        [
            Operation::Include(GenreFilter::new("pentateuch")),
            Operation::Include(BookFilter::new("rev")),
        ],
        6
    );

    mk_test!(
        no_genesis,
        [Operation::Exclude(BookFilter::new("genesis")),],
        65
    );

    mk_test!(
        no_genesis_or_john,
        [
            Operation::Exclude(BookFilter::new("genesis")),
            Operation::Exclude(BookFilter::new("john")),
        ],
        64
    );
}
