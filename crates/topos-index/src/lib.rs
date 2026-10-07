/*!
A compact, persistent index of the Bible references in many files, for apps that search the same
files again and again (a notes vault, a library of books)

- Each file's entry ([`FileEntry`]) keeps its references compactly: the passage and where it is,
  about 15 bytes each. EPUBs keep their CFIs and context apart ([`Detail`]), loaded when needed.
- A file is searched again only when it changed ([`Index::check`]). Entries can come from other
  devices that sync the same files: modification times differ after syncing, so a text file is
  matched by a hash of its contents instead.
- Each device writes its own packs (a binary format, [`Pack`]), so syncing never conflicts.
- [`Index::query`] filters, sorts, and counts in Rust; results are read a page at a time.
*/

mod codec;
pub mod entry;
pub mod index;
pub mod pack;

pub use entry::{
    Detail, EpubEntryBuilder, EpubRef, FileEntry, RefDetail, Reference, Section, Stamp, Unit,
    detail_name, snippet, text_entry, text_hash, text_refs,
};
#[cfg(feature = "epub")]
pub use entry::{epub_entry, epub_file_entry};
pub use index::{Hit, Index, IndexError, Order, PACK_COUNT, Results, Scope, Status};
pub use pack::{Confirmation, EntryMessage, FORMAT, FormatError, Pack, pack_number};
