//! The index of many files' references, as one device sees it

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    sync::Arc,
};

use topos_bible::{
    data::books::BookId,
    segments::{Passage, Segment, verse_bounds::VerseBounds},
};

use crate::{
    entry::{Detail, FileEntry, RefDetail, Reference, Unit},
    pack::{Confirmation, EntryMessage, Pack, pack_number},
};

/// How many packs a device's entries are split into
pub const PACK_COUNT: u32 = 32;

/// How many EPUBs' details stay loaded
const DETAILS_KEPT: usize = 64;

/// Whether the index has a file's references (see [`Index::check`])
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// An entry matches the file's size and modification time
    Fresh,
    /// An entry matches its size but not its time (likely synced from another device): it's
    /// used meanwhile, and [`Index::confirm`] should check it
    Unconfirmed,
    /// No entry matches, so the file needs to be searched
    Missing,
}

/// Which files a query looks at
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    File(String),
    /// Files in a folder (and its subfolders)
    Folder(String),
}

/// How query results are ordered
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// By path, then position in the file
    #[default]
    File,
    /// By where the passage starts in the Bible, then by path and position
    Bible,
}

/// The references a query kept, to read a page at a time with [`Index::page`]
#[derive(Clone, Debug, Default)]
pub struct Results {
    generation: u64,
    /// (slot, reference)
    items: Arc<Vec<(u32, u32)>>,
    files: usize,
}

/// Every reference in use, in an order, while the index is unchanged
#[derive(Debug)]
struct Ordered {
    generation: u64,
    order: Order,
    items: Arc<Vec<(u32, u32)>>,
    files: usize,
}

impl Results {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// How many files the references are in
    pub fn file_count(&self) -> usize {
        self.files
    }
}

/// One reference, with what's known about it
#[derive(Clone, Copy, Debug)]
pub struct Hit<'a> {
    pub path: &'a str,
    pub entry: &'a FileEntry,
    pub reference: &'a Reference,
    /// Its position in the file's references
    pub number: usize,
    /// For EPUBs, when their details are loaded
    pub detail: Option<&'a RefDetail>,
}

impl Hit<'_> {
    pub fn chapter(&self) -> Option<&str> {
        self.entry.chapter(self.reference)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Active {
    Own,
    Other { device: String, written: u64 },
}

/// What the index has for one path
#[derive(Debug, Default)]
struct Slot {
    path: String,
    /// The entry this device made
    own: Option<FileEntry>,
    /// Other devices' entries, by device
    others: BTreeMap<String, FileEntry>,
    /// This device's confirmation of another device's entry
    confirmed: Option<Confirmation>,
    /// The entry in use
    active: Option<Active>,
}

impl Slot {
    fn active_entry(&self) -> Option<&FileEntry> {
        match self.active.as_ref()? {
            Active::Own => self.own.as_ref(),
            Active::Other { device, written } => {
                self.others.get(device).filter(|e| e.written == *written)
            }
        }
    }

    /// Every entry, as (who, entry)
    fn candidates(&self) -> impl Iterator<Item = (Active, &FileEntry)> {
        let own = self.own.iter().map(|e| (Active::Own, e));
        let others = self.others.iter().map(|(device, e)| {
            let who = Active::Other {
                device: device.clone(),
                written: e.written,
            };
            (who, e)
        });
        own.chain(others)
    }
}

/**
The references in many files, from this device's entries and those synced from others

- Files are checked against what the index has with [`Index::check`] (and [`Index::confirm`]),
  and only those that changed need to be searched again
- Each device writes only its own [`Pack`]s ([`Index::take_dirty_packs`]) and reads every
  device's ([`Index::load_pack`]), so syncing never makes two devices write the same file
- Queries filter, sort, and count in Rust, and the results are read a page at a time
*/
#[derive(Debug)]
pub struct Index {
    device: String,
    unit: Unit,
    engine: String,
    slots: Vec<Slot>,
    by_path: HashMap<String, u32>,
    dirty: BTreeSet<u32>,
    details: HashMap<String, Detail>,
    detail_order: VecDeque<String>,
    /// Details not written yet (never dropped before they are)
    unsaved: BTreeSet<String>,
    generation: u64,
    /// Every reference's order, by [`Order`], for queries (see [`Index::query`])
    ordered: RefCell<Vec<Arc<Ordered>>>,
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum IndexError {
    #[error("the pack counts offsets in {0:?}, and this index in {1:?}")]
    Unit(Unit, Unit),
    #[error("the pack splits paths {0} ways, and this index {PACK_COUNT}")]
    PackCount(u32),
}

impl Index {
    /// `device` names this device's packs; `engine` (like a version) is written in them
    pub fn new(device: &str, unit: Unit, engine: &str) -> Self {
        Self {
            device: device.to_string(),
            unit,
            engine: engine.to_string(),
            slots: vec![],
            by_path: HashMap::new(),
            dirty: BTreeSet::new(),
            details: HashMap::new(),
            detail_order: VecDeque::new(),
            unsaved: BTreeSet::new(),
            generation: 0,
            ordered: RefCell::new(vec![]),
        }
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn unit(&self) -> Unit {
        self.unit
    }

    /// Changes whenever the references in use change
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn slot_id(&mut self, path: &str) -> u32 {
        if let Some(&id) = self.by_path.get(path) {
            return id;
        }
        let id = self.slots.len() as u32;
        self.slots.push(Slot {
            path: path.to_string(),
            ..Slot::default()
        });
        self.by_path.insert(path.to_string(), id);
        id
    }

    fn slot(&self, path: &str) -> Option<&Slot> {
        self.slots.get(*self.by_path.get(path)? as usize)
    }

    fn slot_mut(&mut self, path: &str) -> Option<&mut Slot> {
        let id = *self.by_path.get(path)?;
        self.slots.get_mut(id as usize)
    }

    fn touch(&mut self, path: &str) {
        self.dirty.insert(pack_number(path, PACK_COUNT));
        self.generation += 1;
    }

    /**
    Adds a device's pack: this device's own (from an earlier run) or another's (synced). It
    replaces what the index had from that device's pack of the same number. Files whose entry
    changed should be checked again.
    */
    pub fn load_pack(&mut self, pack: Pack) -> Result<(), IndexError> {
        if pack.unit != self.unit {
            return Err(IndexError::Unit(pack.unit, self.unit));
        }
        if pack.count != PACK_COUNT {
            return Err(IndexError::PackCount(pack.count));
        }
        let own = pack.device == self.device;
        for slot in &mut self.slots {
            if pack_number(&slot.path, PACK_COUNT) != pack.number {
                continue;
            }
            if own {
                slot.own = None;
                slot.confirmed = None;
            } else {
                slot.others.remove(&pack.device);
            }
        }
        for (path, entry) in pack.entries {
            let id = self.slot_id(&path) as usize;
            let slot = &mut self.slots[id];
            if own {
                slot.own = Some(entry);
            } else {
                slot.others.insert(pack.device.clone(), entry);
            }
        }
        if own {
            for (path, confirmation) in pack.confirmations {
                let id = self.slot_id(&path) as usize;
                self.slots[id].confirmed = Some(confirmation);
            }
        }
        self.generation += 1;
        Ok(())
    }

    /**
    Whether the index has the references of a file with this size and modification time (in
    milliseconds), and makes that entry the one in use
    - The time can differ after syncing, so an entry of the same size is used meanwhile
      ([`Status::Unconfirmed`]) until [`Index::confirm`] checks it
    */
    pub fn check(&mut self, path: &str, size: u64, mtime: u64) -> Status {
        let Some(slot) = self.slot_mut(path) else {
            return Status::Missing;
        };
        let before = slot.active.clone();
        let exact = |e: &FileEntry| e.stamp.size == size && e.stamp.mtime == mtime;
        let confirmed = slot.confirmed.as_ref().and_then(|c| {
            let current = c.size == size && c.mtime == mtime;
            let entry = slot
                .others
                .get(&c.device)
                .filter(|e| e.written == c.written);
            (current && entry.is_some()).then(|| Active::Other {
                device: c.device.clone(),
                written: c.written,
            })
        });
        let newest_exact = slot
            .candidates()
            .filter(|(_, e)| exact(e))
            .max_by_key(|(_, e)| e.written)
            .map(|(who, _)| who);
        let (active, status) = if slot.own.as_ref().is_some_and(exact) {
            (Some(Active::Own), Status::Fresh)
        } else if let Some(who) = confirmed.or(newest_exact) {
            (Some(who), Status::Fresh)
        } else {
            let same_size = slot
                .candidates()
                .filter(|(_, e)| e.stamp.size == size)
                .max_by_key(|(_, e)| e.written)
                .map(|(who, _)| who);
            match same_size {
                Some(who) => (Some(who), Status::Unconfirmed),
                None => (None, Status::Missing),
            }
        };
        slot.active = active;
        if slot.active != before {
            self.generation += 1;
        }
        status
    }

    /**
    Checks an [`Status::Unconfirmed`] file: an entry of the same size is the file's if its
    content hash is `hash` ([`crate::text_hash`]), or with no hash (for files too big to read
    again, like EPUBs) if any entry has the same size. Returns whether one did; if none did, the
    file needs to be searched.
    */
    pub fn confirm(&mut self, path: &str, size: u64, mtime: u64, hash: Option<u64>) -> bool {
        let Some(slot) = self.slot_mut(path) else {
            return false;
        };
        let matches =
            |e: &FileEntry| e.stamp.size == size && (hash.is_none() || e.stamp.hash == hash);
        let best = slot
            .candidates()
            .filter(|(_, e)| matches(e))
            .max_by_key(|(_, e)| e.written)
            .map(|(who, e)| (who, e.written));
        match best {
            Some((Active::Own, _)) => {
                if let Some(own) = &mut slot.own {
                    own.stamp.mtime = mtime;
                }
                slot.active = Some(Active::Own);
            }
            Some((Active::Other { device, written }, _)) => {
                slot.confirmed = Some(Confirmation {
                    size,
                    mtime,
                    device: device.clone(),
                    written,
                });
                slot.active = Some(Active::Other { device, written });
            }
            None => {
                slot.active = None;
                self.generation += 1;
                return false;
            }
        }
        self.touch(path);
        true
    }

    /// Sets a file's references (from a search on this device)
    pub fn set(&mut self, path: &str, entry: FileEntry) {
        let id = self.slot_id(path) as usize;
        let slot = &mut self.slots[id];
        slot.own = Some(entry);
        slot.confirmed = None;
        slot.active = Some(Active::Own);
        self.touch(path);
    }

    /// Sets a file's references and details (from [`EntryMessage::encode`]), and returns its path
    pub fn insert(&mut self, message: EntryMessage) -> Result<String, IndexError> {
        if message.unit != self.unit {
            return Err(IndexError::Unit(message.unit, self.unit));
        }
        if let (Some(name), Some(detail)) = (&message.entry.detail, message.detail) {
            self.unsaved.insert(name.clone());
            self.add_detail(name, detail);
        }
        self.set(&message.path, message.entry);
        Ok(message.path)
    }

    /// Takes a file out of the index (deleted, or no longer searched)
    pub fn remove(&mut self, path: &str) {
        let Some(slot) = self.slot_mut(path) else {
            return;
        };
        let changed = slot.own.is_some() || slot.confirmed.is_some();
        let was_active = slot.active.take().is_some();
        slot.own = None;
        slot.confirmed = None;
        if changed {
            self.touch(path);
        } else if was_active {
            self.generation += 1;
        }
    }

    /// Moves a file's references to its new path
    pub fn rename(&mut self, old: &str, new: &str) {
        let entry = self.slot(old).and_then(Slot::active_entry).cloned();
        self.remove(old);
        if let Some(entry) = entry {
            self.set(new, entry);
        }
    }

    /// Takes every file not in `paths` out of the index (see [`Index::remove`])
    pub fn retain(&mut self, paths: &HashSet<&str>) {
        let gone: Vec<String> = self
            .slots
            .iter()
            .filter(|s| !paths.contains(s.path.as_str()))
            .filter(|s| s.own.is_some() || s.confirmed.is_some() || s.active.is_some())
            .map(|s| s.path.clone())
            .collect();
        for path in gone {
            self.remove(&path);
        }
    }

    /// Whether this device has changes to write
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// This device's packs that changed, to write (as `<device>-<number>`), and forgets that
    /// they changed
    pub fn take_dirty_packs(&mut self) -> Vec<Pack> {
        let numbers = std::mem::take(&mut self.dirty);
        numbers.into_iter().map(|n| self.pack(n)).collect()
    }

    /// This device's pack `number`, with every entry and confirmation in it
    pub fn pack(&self, number: u32) -> Pack {
        let mut entries = vec![];
        let mut confirmations = vec![];
        for slot in &self.slots {
            if pack_number(&slot.path, PACK_COUNT) != number {
                continue;
            }
            if let Some(entry) = &slot.own {
                entries.push((slot.path.clone(), entry.clone()));
            }
            if let Some(confirmation) = &slot.confirmed {
                confirmations.push((slot.path.clone(), confirmation.clone()));
            }
        }
        Pack {
            device: self.device.clone(),
            number,
            count: PACK_COUNT,
            unit: self.unit,
            engine: self.engine.clone(),
            entries,
            confirmations,
        }
    }

    // Details

    fn add_detail(&mut self, name: &str, detail: Detail) {
        if self.details.insert(name.to_string(), detail).is_none() {
            self.detail_order.push_back(name.to_string());
        }
        // Drop the oldest loaded details (never unsaved ones)
        while self.details.len() > DETAILS_KEPT {
            let Some(position) = self
                .detail_order
                .iter()
                .position(|n| !self.unsaved.contains(n) && n != name)
            else {
                break;
            };
            if let Some(old) = self.detail_order.remove(position) {
                self.details.remove(&old);
            }
        }
    }

    /// Adds an EPUB's details (read from where [`Index::take_unsaved_details`] wrote them)
    pub fn load_detail(&mut self, name: &str, detail: Detail) {
        self.add_detail(name, detail);
        self.generation += 1;
    }

    pub fn has_detail(&self, name: &str) -> bool {
        self.details.contains_key(name)
    }

    /// New details to write, by name, and forgets that they need writing
    pub fn take_unsaved_details(&mut self) -> Vec<(String, Detail)> {
        let names = std::mem::take(&mut self.unsaved);
        names
            .into_iter()
            .filter_map(|name| {
                let detail = self.details.get(&name)?.clone();
                Some((name, detail))
            })
            .collect()
    }

    /// The details in use, by name, so the others can be deleted
    pub fn detail_names(&self) -> BTreeSet<String> {
        self.slots
            .iter()
            .flat_map(|s| s.candidates().filter_map(|(_, e)| e.detail.clone()))
            .collect()
    }

    // Reading

    /// The entry in use for a file
    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.slot(path)?.active_entry()
    }

    fn hit(&self, slot: u32, number: u32) -> Option<Hit<'_>> {
        let slot = self.slots.get(slot as usize)?;
        let entry = slot.active_entry()?;
        let reference = entry.refs.get(number as usize)?;
        let detail = entry
            .detail
            .as_ref()
            .and_then(|name| self.details.get(name))
            .and_then(|d| d.refs.get(number as usize));
        Some(Hit {
            path: &slot.path,
            entry,
            reference,
            number: number as usize,
            detail,
        })
    }

    /// A file's references in use
    pub fn file_hits(&self, path: &str) -> Vec<Hit<'_>> {
        let Some(&id) = self.by_path.get(path) else {
            return vec![];
        };
        let count = self.entry(path).map_or(0, |e| e.refs.len());
        (0..count as u32).filter_map(|n| self.hit(id, n)).collect()
    }

    /// The files with references, by path
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.slots
            .iter()
            .filter(|s| s.active_entry().is_some_and(|e| !e.refs.is_empty()))
            .map(|s| s.path.as_str())
    }

    pub fn file_count(&self) -> usize {
        self.paths().count()
    }

    pub fn reference_count(&self) -> usize {
        self.slots
            .iter()
            .filter_map(Slot::active_entry)
            .map(|e| e.refs.len())
            .sum()
    }

    /**
    The references in `scope` that `keep` keeps (all of them with [`None`]), in `order`
    - `books`: the only books `keep` can keep, if it knows (like
      `BibleMatcher::possible_books`), so references in others are skipped quickly
    - Every reference's order is kept until the index changes, so queries only filter it
    */
    pub fn query(
        &self,
        keep: Option<&dyn Fn(&Passage) -> bool>,
        books: Option<&BTreeSet<BookId>>,
        scope: &Scope,
        order: Order,
    ) -> Results {
        if keep.is_none() && books.is_none() && *scope == Scope::All {
            let all = self.ordered(order);
            return Results {
                generation: self.generation,
                items: all.items.clone(),
                files: all.files,
            };
        }
        let in_scope: Vec<bool> = self
            .slots
            .iter()
            .map(|slot| match scope {
                Scope::All => true,
                Scope::File(file) => slot.path == *file,
                Scope::Folder(folder) => {
                    let folder = folder.trim_end_matches('/');
                    folder.is_empty()
                        || slot
                            .path
                            .strip_prefix(folder)
                            .is_some_and(|rest| rest.starts_with('/'))
                }
            })
            .collect();
        // References in books the filter can't keep are skipped without testing them
        let mut book_kept = [true; 256];
        if let Some(books) = books {
            book_kept = [false; 256];
            for book in books {
                book_kept[usize::from(book.0)] = true;
            }
        }
        // Filtered in file order (which reads memory in order), then sorted if needed
        let entries: Vec<Option<&FileEntry>> = self.slots.iter().map(Slot::active_entry).collect();
        let mut has_file = vec![false; self.slots.len()];
        let mut kept: Vec<(u64, (u32, u32))> = vec![];
        for (position, &(slot, number)) in self.ordered(Order::File).items.iter().enumerate() {
            let (s, n) = (slot as usize, number as usize);
            if !in_scope[s] {
                continue;
            }
            let Some(reference) = entries[s].and_then(|e| e.refs.get(n)) else {
                continue;
            };
            let passage = &reference.passage;
            if book_kept[usize::from(passage.book.0)] && keep.is_none_or(|keep| keep(passage)) {
                has_file[s] = true;
                let key = match order {
                    Order::Bible => (u64::from(bible_key(passage)) << 32) | position as u64,
                    Order::File => 0,
                };
                kept.push((key, (slot, number)));
            }
        }
        if order == Order::Bible {
            // Keys are unique (they end with the file order), so an unstable sort is enough
            kept.sort_unstable_by_key(|&(key, _)| key);
        }
        Results {
            generation: self.generation,
            items: Arc::new(kept.into_iter().map(|(_, item)| item).collect()),
            files: has_file.iter().filter(|&&f| f).count(),
        }
    }

    /// Every reference in use, in an order (kept until the index changes)
    fn ordered(&self, order: Order) -> Arc<Ordered> {
        let mut cache = self.ordered.borrow_mut();
        cache.retain(|o| o.generation == self.generation);
        if let Some(found) = cache.iter().find(|o| o.order == order) {
            return found.clone();
        }
        // Paths in order, compared once each by a lowercase key
        let mut slots: Vec<(String, u32)> = (0..self.slots.len() as u32)
            .map(|id| (lowercase(&self.slots[id as usize].path), id))
            .collect();
        slots.sort_by(|(a_key, a), (b_key, b)| {
            a_key.cmp(b_key).then_with(|| {
                self.slots[*a as usize]
                    .path
                    .cmp(&self.slots[*b as usize].path)
            })
        });
        // (Bible order key, slot, reference), in file order
        let mut found: Vec<(u32, u32, u32)> = vec![];
        let mut files = 0;
        for (_, id) in &slots {
            let Some(entry) = self.slots[*id as usize].active_entry() else {
                continue;
            };
            files += usize::from(!entry.refs.is_empty());
            for (number, reference) in entry.refs.iter().enumerate() {
                let key = match order {
                    Order::Bible => bible_key(&reference.passage),
                    Order::File => 0,
                };
                found.push((key, *id, number as u32));
            }
        }
        if order == Order::Bible {
            // By passage, then file order (each item's position), so keys are unique and an
            // unstable sort is enough
            let mut keyed: Vec<u64> = found
                .iter()
                .enumerate()
                .map(|(i, &(key, _, _))| (u64::from(key) << 32) | i as u64)
                .collect();
            keyed.sort_unstable();
            found = keyed
                .into_iter()
                .map(|key| found[(key & 0xffff_ffff) as usize])
                .collect();
        }
        let ordered = Arc::new(Ordered {
            generation: self.generation,
            order,
            items: Arc::new(found.into_iter().map(|(_, id, n)| (id, n)).collect()),
            files,
        });
        cache.push(ordered.clone());
        ordered
    }

    /// Results `offset` to `offset + limit` (any that changed since the query are left out)
    pub fn page(&self, results: &Results, offset: usize, limit: usize) -> Vec<Hit<'_>> {
        results
            .items
            .iter()
            .skip(offset)
            .take(limit)
            .filter_map(|&(slot, number)| self.hit(slot, number))
            .collect()
    }

    /// Whether the index changed since the query
    pub fn is_current(&self, results: &Results) -> bool {
        results.generation == self.generation
    }

    /// The details the page needs that aren't loaded
    pub fn missing_details(&self, results: &Results, offset: usize, limit: usize) -> Vec<String> {
        let mut names: Vec<String> = self
            .page(results, offset, limit)
            .iter()
            .filter_map(|hit| hit.entry.detail.clone())
            .filter(|name| !self.details.contains_key(name))
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// For ordering paths case-insensitively, like a file list
fn lowercase(path: &str) -> String {
    path.chars().flat_map(char::to_lowercase).collect()
}

/// Where a passage starts in the Bible (a whole chapter before its first verse)
fn bible_key(passage: &Passage) -> u32 {
    let Some(first) = passage.segments.first() else {
        return u32::from(passage.book.0) << 16;
    };
    let verse = match first {
        Segment::FullChapter(_)
        | Segment::FullChapterRange(_)
        | Segment::FullChapterVerseRange(_) => 0,
        _ => first.starting_verse(),
    };
    (u32::from(passage.book.0) << 16)
        | (u32::from(first.starting_chapter()) << 8)
        | u32::from(verse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Stamp, text_entry, text_hash};
    use topos_bible::matcher::BibleMatcher;

    fn entry(text: &str, size: u64, mtime: u64, written: u64) -> FileEntry {
        text_entry(
            &BibleMatcher::default(),
            text,
            Unit::Utf16,
            size,
            mtime,
            written,
        )
    }

    fn references(index: &Index, path: &str) -> Vec<String> {
        index
            .file_hits(path)
            .iter()
            .map(|h| h.reference.passage.segments.to_string())
            .collect()
    }

    #[test]
    fn entries_are_used_while_the_file_is_unchanged() {
        let mut index = Index::new("laptop", Unit::Utf16, "test");
        assert_eq!(index.check("a.md", 10, 1), Status::Missing);
        index.set("a.md", entry("Jn 3:16", 10, 1, 100));
        assert_eq!(index.check("a.md", 10, 1), Status::Fresh);
        assert_eq!(references(&index, "a.md"), ["3:16"]);
        // Touched: same size, so checked by hash
        assert_eq!(index.check("a.md", 10, 2), Status::Unconfirmed);
        assert!(index.confirm("a.md", 10, 2, Some(text_hash("Jn 3:16"))));
        assert_eq!(index.check("a.md", 10, 2), Status::Fresh);
        // Edited
        assert_eq!(index.check("a.md", 11, 3), Status::Missing);
        assert!(index.file_hits("a.md").is_empty());
    }

    #[test]
    fn another_devices_entries_are_confirmed_once() {
        let mut laptop = Index::new("laptop", Unit::Utf16, "test");
        laptop.set("a.md", entry("Rom 8:28", 8, 1, 100));
        laptop.set("b.epub", entry("Gen 1:1", 500, 1, 100));
        let packs = laptop.take_dirty_packs();
        assert!(!packs.is_empty());
        assert!(laptop.take_dirty_packs().is_empty());

        let mut phone = Index::new("phone", Unit::Utf16, "test");
        for pack in packs.iter().map(|p| Pack::decode(&p.encode()).unwrap()) {
            phone.load_pack(pack).unwrap();
        }
        // Synced files have other times here
        assert_eq!(phone.check("a.md", 8, 50), Status::Unconfirmed);
        assert_eq!(references(&phone, "a.md"), ["8:28"], "used meanwhile");
        assert!(!phone.confirm("a.md", 8, 50, Some(text_hash("Rom 8:29"))));
        assert!(references(&phone, "a.md").is_empty());
        assert_eq!(phone.check("a.md", 8, 50), Status::Unconfirmed);
        assert!(phone.confirm("a.md", 8, 50, Some(text_hash("Rom 8:28"))));
        // EPUBs by size
        assert_eq!(phone.check("b.epub", 500, 60), Status::Unconfirmed);
        assert!(phone.confirm("b.epub", 500, 60, None));
        assert_eq!(phone.check("b.epub", 500, 60), Status::Fresh);

        // The confirmations are the phone's to keep, so the next start is quick
        let mut again = Index::new("phone", Unit::Utf16, "test");
        for pack in packs.into_iter().chain(phone.take_dirty_packs()) {
            again.load_pack(pack).unwrap();
        }
        assert_eq!(again.check("a.md", 8, 50), Status::Fresh);
        assert_eq!(again.check("b.epub", 500, 60), Status::Fresh);
        assert_eq!(again.check("b.epub", 501, 60), Status::Missing);
    }

    #[test]
    fn a_reloaded_pack_replaces_what_it_had() {
        let mut laptop = Index::new("laptop", Unit::Utf16, "test");
        laptop.set("a.md", entry("Jn 1:1", 6, 1, 100));
        let number = pack_number("a.md", PACK_COUNT);
        let mut phone = Index::new("phone", Unit::Utf16, "test");
        phone.load_pack(laptop.pack(number)).unwrap();
        laptop.remove("a.md");
        phone.load_pack(laptop.pack(number)).unwrap();
        assert_eq!(phone.check("a.md", 6, 1), Status::Missing);
        assert_eq!(
            phone.load_pack(Pack {
                unit: Unit::Byte,
                ..laptop.pack(number)
            }),
            Err(IndexError::Unit(Unit::Byte, Unit::Utf16))
        );
    }

    #[test]
    fn queries_filter_sort_and_page() {
        let mut index = Index::new("laptop", Unit::Utf16, "test");
        index.set("Notes/b.md", entry("Rom 8:28 and Gen 1:1", 1, 1, 1));
        index.set("Notes/A.md", entry("John 3:16", 1, 1, 1));
        index.set("other.md", entry("Gen 2:1", 1, 1, 1));
        let written = |hits: Vec<Hit>| {
            hits.iter()
                .map(|h| format!("{} {}", h.path, h.reference.passage.segments))
                .collect::<Vec<_>>()
        };
        let all = index.query(None, None, &Scope::All, Order::File);
        assert_eq!((all.len(), all.file_count()), (4, 3));
        assert_eq!(
            written(index.page(&all, 0, 10)),
            [
                "Notes/A.md 3:16",
                "Notes/b.md 8:28",
                "Notes/b.md 1:1",
                "other.md 2:1"
            ]
        );
        let bible = index.query(None, None, &Scope::Folder("Notes/".into()), Order::Bible);
        assert_eq!(
            written(index.page(&bible, 1, 2)),
            ["Notes/A.md 3:16", "Notes/b.md 8:28"]
        );
        let genesis = index.query(
            Some(&|p: &Passage| p.book.0 == 1),
            None,
            &Scope::All,
            Order::Bible,
        );
        assert_eq!(
            written(index.page(&genesis, 0, 10)),
            ["Notes/b.md 1:1", "other.md 2:1"]
        );
        assert!(index.is_current(&genesis));
        index.rename("other.md", "moved.md");
        assert!(!index.is_current(&genesis));
        assert_eq!(references(&index, "moved.md"), ["2:1"]);
        assert_eq!((index.file_count(), index.reference_count()), (3, 4));
        index.retain(&HashSet::from(["moved.md"]));
        assert_eq!(index.file_count(), 1);
    }

    #[test]
    fn details_load_on_demand() {
        let mut index = Index::new("laptop", Unit::Utf16, "test");
        let stamp = Stamp {
            size: 9,
            mtime: 1,
            hash: None,
        };
        let mut builder = crate::entry::EpubEntryBuilder::default();
        builder.push(crate::entry::EpubRef {
            passage: BibleMatcher::default().search("Jn 3:16")[0].psg.clone(),
            start: 40,
            end: 47,
            line: 3,
            column: 5,
            spine: 2,
            chapter: Some("One"),
            cfi: "epubcfi(/6/6!/4/2,/1:4,/1:11)".into(),
            line_text: "See Jn 3:16.",
        });
        let (entry, detail) = builder.finish(stamp, 1, "d1".into());
        let message = EntryMessage {
            path: "b.epub".into(),
            unit: Unit::Utf16,
            entry,
            detail: Some(detail.clone()),
        };
        index
            .insert(EntryMessage::decode(&message.encode()).unwrap())
            .unwrap();
        let hits = index.file_hits("b.epub");
        assert_eq!(hits[0].chapter(), Some("One"));
        assert_eq!(hits[0].detail.map(|d| d.at), Some(4));
        let saved = index.take_unsaved_details();
        assert_eq!(saved, [("d1".to_string(), detail.clone())]);

        // Another device has the entry, and loads the details when shown
        let mut phone = Index::new("phone", Unit::Utf16, "test");
        for pack in index.take_dirty_packs() {
            phone.load_pack(pack).unwrap();
        }
        assert_eq!(phone.check("b.epub", 9, 1), Status::Fresh);
        let results = phone.query(None, None, &Scope::All, Order::File);
        assert_eq!(phone.missing_details(&results, 0, 10), ["d1"]);
        phone.load_detail("d1", Detail::decode(&detail.encode()).unwrap());
        assert!(phone.missing_details(&results, 0, 10).is_empty());
        assert_eq!(
            phone.page(&results, 0, 1)[0].detail.map(|d| d.cfi.as_str()),
            Some("epubcfi(/6/6!/4/2,/1:4,/1:11)")
        );
        assert_eq!(phone.detail_names(), BTreeSet::from(["d1".to_string()]));
    }
}
