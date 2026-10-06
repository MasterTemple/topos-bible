use std::ops::Bound;

use crate::segments::segment::Segment;

pub trait VerseBounds: Copy + Sized + std::fmt::Debug + Into<Segment> {
    fn starting_verse(&self) -> u8;

    fn starting_chapter(&self) -> u8;

    fn ending_verse(&self) -> Option<u8>;

    fn ending_chapter(&self) -> u8;

    /// - The verse range starts at 1 when not the starting chapter
    /// - The verse range is unbounded when not the ending chapter
    fn verse_range(&self, chapter: u8) -> (Bound<u8>, Bound<u8>) {
        let start_bound = if chapter == self.starting_chapter() {
            Bound::Included(self.starting_verse())
        } else {
            Bound::Included(1)
        };
        let end_bound = if chapter == self.ending_chapter() {
            match self.ending_verse() {
                Some(ending_verse) => Bound::Included(ending_verse),
                None => Bound::Unbounded,
            }
        } else {
            Bound::Unbounded
        };
        (start_bound, end_bound)
    }

    fn chapter_range(&self) -> std::ops::RangeInclusive<u8> {
        self.starting_chapter()..=self.ending_chapter()
    }

    fn ends_before(&self, other: &impl VerseBounds) -> bool {
        // it finishes in a chapter before the other one
        self.ending_chapter() < other.starting_chapter()
        // or it is in the same chapter and this ending verse < other starting verse
        || (
            self.ending_chapter() == other.starting_chapter()
            && self.ending_verse().is_some_and(|ending_verse| ending_verse < other.starting_verse())
        )
    }

    fn ends_at_or_before(&self, other: &impl VerseBounds) -> bool {
        // it finishes in a chapter before the other one
        self.ending_chapter() < other.starting_chapter()
        // or it is in the same chapter and this ending verse < other starting verse
        || (
            self.ending_chapter() == other.starting_chapter()
            && self.ending_verse().is_some_and(|ending_verse| ending_verse <= other.starting_verse())
        )
    }

    fn starts_after(&self, other: &impl VerseBounds) -> bool {
        other.ends_before(self)
    }

    fn starts_at_or_after(&self, other: &impl VerseBounds) -> bool {
        other.ends_at_or_before(self)
    }

    // If:
    // - This segment ends before the other segment starts
    // OR
    // - This segment starts after the other segment ends
    // Then:
    // - This segment does NOT overlap with the other segment
    fn overlaps_with(&self, other: &impl VerseBounds) -> bool {
        !(self.ends_before(other) || self.starts_after(other))
    }

    fn fully_contains(&self, other: &impl VerseBounds) -> bool {
        (
            // starting chapter is before
            self.starting_chapter() < other.starting_chapter()
            // or starting chapter is same and starting verse is before
            || (
                // starting chapter is same
                self.starting_chapter() == other.starting_chapter()
                // starting verse is before
                && self.starting_verse() <= other.starting_verse()
            )
        ) && (
            // ending chapter is before
            other.ending_chapter() < self.ending_chapter()
            // or ending chapter is same and ending verse is after
            || (
                // ending chapter is same
                self.ending_chapter() == other.ending_chapter()
                // ending verse is before
                && match (other.ending_verse(), self.ending_verse()) {
                    // perform comparison
                    (Some(other), Some(this)) => other <= this,
                    // other is unbounded, self is bounded => self doesn't contain other
                    // TODO: what about when the user specifies 1 John 1:1-10 and 1 John 1 is found,
                    // since they cover the same range
                    (None, Some(_)) => false,
                    // other is bounded, self is unbounded => self contain other
                    (Some(_), None) => true,
                    // both are chapter ends
                    (None, None) => true,
                }
            )
        )
    }

    // /// determines what kind of passage segment this really is
    // fn actual(&self) -> Segment {
    //     let starting_chapter = self.starting_chapter();
    //     let starting_verse = self.starting_verse();
    //     let ending_chapter = self.ending_chapter();
    //     let same_chapter = starting_chapter == ending_chapter;
    //
    //     if let Some(ending_verse) = self.ending_verse() {
    //         // it must be either a chapter verse or a chapter verse range
    //         if same_chapter {
    //             if starting_verse == ending_verse {
    //                 Segment::ChapterVerse(ChapterVerse::new(starting_chapter, starting_verse))
    //             }
    //             else {
    //                 Segment::ChapterVerseRange(ChapterVerseRange::new(starting_chapter, starting_verse, ending_verse))
    //             }
    //
    //         }
    //         // it must be a chapter range
    //         else {
    //             Segment::ChapterRange(ChapterRange::new(starting_chapter, starting_verse, ending_chapter, ending_verse))
    //         }
    //     }
    //     // it must be a full chapter or a full chapter range
    //     else {
    //         if same_chapter {
    //             Segment::FullChapter(FullChapter::new(starting_chapter))
    //         } else {
    //             Segment::FullChapterRange(FullChapterRange::new(starting_chapter, ending_chapter))
    //         }
    //     }
    // }
    //
    // fn with_content<'a, Content>(&'_ self, content: &'a Content) -> PassageContent<'a, Self, Content> {
    //     PassageContent {
    //         segment: *self,
    //         content
    //     }
    // }
    //
    // fn with_book(&self, book: u8) -> BookSegment<Self> {
    //     BookSegment {
    //         book,
    //         segment: *self,
    //     }
    // }
}

#[cfg(test)]
mod tests {
    use crate::{
        error::AnyResult,
        segments::{segment::Segment, verse_bounds::VerseBounds},
    };

    #[test]
    fn fully_contains() -> AnyResult<()> {
        let psg = |s: &str| Segment::parse(s);

        assert!(psg("2:2")?.fully_contains(&psg("2:2")?));
        assert!(psg("2:1-2")?.fully_contains(&psg("2:2")?));
        assert!(psg("2:2-3")?.fully_contains(&psg("2:2")?));
        assert!(psg("2:1-3")?.fully_contains(&psg("2:2")?));

        assert!(psg("2:1-2")?.fully_contains(&psg("2:1-2")?));
        assert!(!psg("2:2-3")?.fully_contains(&psg("2:1-2")?));
        assert!(psg("2:1-3")?.fully_contains(&psg("2:1-2")?));

        assert!(!psg("2:1-2")?.fully_contains(&psg("2")?));
        assert!(psg("2")?.fully_contains(&psg("2:1-2")?));

        Ok(())
    }
}
