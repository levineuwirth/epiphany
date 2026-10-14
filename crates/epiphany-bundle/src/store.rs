//! The block-store abstraction and its implementations.
//!
//! The bundle is a single file accessed at byte offsets with an explicit
//! *durable flush* at the commit point (Chapter 8 §"Durable Writes": the flush
//! is `fsync`/`FlushFileBuffers`/equivalent — *"durability MUST be requested
//! explicitly at the commit point"*; implementations *"MAY NOT rely on
//! filesystem ordering guarantees or write-back caching alone"*).
//!
//! [`BlockStore`] captures exactly that contract: positioned reads/writes plus a
//! `flush` that makes prior writes durable. Four implementations:
//!
//! * [`MemStore`] — a plain in-memory image (`flush` is a no-op; the bytes *are*
//!   durable). The default for tests and for the cold-open path over an image.
//! * [`FileStore`] — a real file whose `flush` calls `fsync`. Demonstrates the
//!   atomic-commit protocol against a real filesystem.
//! * [`FaultStore`] — a crash simulator that distinguishes *live* (page-cache)
//!   bytes from *durable* (survives-a-crash) bytes and can "crash" after any
//!   syscall, optionally tearing the in-flight write. This is the engine behind
//!   the crash-recovery fuzzer — Agent D's acceptance gate.
//! * [`PageCacheStore`] — a failed `fsync` the process lives through, as Linux
//!   filesystems report one ([`FailedFlush`]): what a writer reads, and what is
//!   on disk, once a flush has failed and the store is read and flushed again.

use std::cell::{Cell, RefCell};
use std::io;

/// A positioned byte store with an explicit durability boundary.
///
/// Writes are not guaranteed durable until [`BlockStore::flush`] returns
/// successfully. A crash (process death) loses any write not yet covered by a
/// successful flush. This is the contract the atomic-commit protocol is built
/// on, and the contract [`FaultStore`] adversarially exercises.
pub trait BlockStore {
    /// The current length of the store, in bytes.
    fn len(&self) -> u64;

    /// Whether the store is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads exactly `buf.len()` bytes starting at `offset`. Errors (rather than
    /// short-reads or panics) if the range runs past the end of the store.
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;

    /// Writes `data` starting at `offset`, extending the store (zero-filling any
    /// gap) as needed. The write is *not* durable until [`Self::flush`].
    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()>;

    /// Makes all previously-written bytes durable (the platform `fsync`).
    fn flush(&mut self) -> io::Result<()>;
}

/// A boxed store is a store, so one owner can hold a file-backed or in-memory
/// bundle behind one type (an editor window holding either a saved document or
/// an import not yet saved).
impl<S: BlockStore + ?Sized> BlockStore for Box<S> {
    fn len(&self) -> u64 {
        (**self).len()
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        (**self).read_exact_at(offset, buf)
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        (**self).write_at(offset, data)
    }

    fn flush(&mut self) -> io::Result<()> {
        (**self).flush()
    }
}

/// Reads exactly `len` bytes at `offset` into a fresh `Vec`.
pub(crate) fn read_vec(store: &dyn BlockStore, offset: u64, len: u64) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; len as usize];
    store.read_exact_at(offset, &mut buf)?;
    Ok(buf)
}

fn out_of_range() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "read past end of block store")
}

/// Applies a positioned write to an in-memory byte image, extending and
/// zero-filling as needed.
fn apply_write(image: &mut Vec<u8>, offset: u64, data: &[u8]) {
    let end = offset as usize + data.len();
    if image.len() < end {
        image.resize(end, 0);
    }
    image[offset as usize..end].copy_from_slice(data);
}

// --------------------------------------------------------------------------
// MemStore
// --------------------------------------------------------------------------

/// An in-memory block store. `flush` is a no-op: the bytes are the durable
/// truth. The natural backing for tests, for opening a recovered crash image,
/// and (per QUICKSTART decision 3) for v0's in-memory bundle.
#[derive(Clone, Default)]
pub struct MemStore {
    bytes: Vec<u8>,
}

impl MemStore {
    /// A fresh empty store.
    pub fn new() -> Self {
        MemStore { bytes: Vec::new() }
    }

    /// A store over an existing byte image (e.g. a recovered crash image).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        MemStore { bytes }
    }

    /// The store's bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Consumes the store, returning its bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl BlockStore for MemStore {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset as usize + buf.len();
        if end > self.bytes.len() {
            return Err(out_of_range());
        }
        buf.copy_from_slice(&self.bytes[offset as usize..end]);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        apply_write(&mut self.bytes, offset, data);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// --------------------------------------------------------------------------
// FileStore (real fsync)
// --------------------------------------------------------------------------

/// A block store backed by a real file, whose [`BlockStore::flush`] issues a
/// `fsync`. This is the production durability path; the crash fuzzer uses
/// [`FaultStore`] instead because a real `fsync` cannot be interrupted
/// deterministically in a test.
///
/// Unix-only: it uses positioned `pread`/`pwrite` (`FileExt`) so reads need no
/// `&mut`. The crash-recovery gate does not depend on this type.
#[cfg(unix)]
pub struct FileStore {
    file: std::fs::File,
    /// The current file length, tracked in memory. Read once (fallibly) at
    /// construction and maintained on every write, so [`BlockStore::len`] is
    /// infallible and never collapses a transient metadata error to `0` — which
    /// would let a later commit's append overwrite the prelude.
    len: u64,
}

#[cfg(unix)]
impl FileStore {
    /// Creates (or truncates) a bundle file at `path`.
    pub fn create(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(FileStore { file, len: 0 })
    }

    /// Creates a bundle file at `path`, refusing with
    /// [`io::ErrorKind::AlreadyExists`] when a file is already there: creating a
    /// document never truncates another (`spec/PLAN_EDITOR_APP.md` §Ruling B,
    /// "creation uses `create_new`"). [`FileStore::create`] truncates.
    pub fn create_new(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        Ok(FileStore { file, len: 0 })
    }

    /// Opens an existing bundle file for read/write.
    pub fn open(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        // Establish the length once, propagating any metadata error here rather
        // than masking it in `len()`.
        let len = file.metadata()?.len();
        Ok(FileStore { file, len })
    }
}

#[cfg(unix)]
impl BlockStore for FileStore {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset)
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.write_all_at(data, offset)?;
        self.len = self.len.max(offset + data.len() as u64);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        // sync_all (fsync) makes both data and the file-size metadata durable;
        // the latter matters because commits extend the file.
        self.file.sync_all()
    }
}

// --------------------------------------------------------------------------
// FaultStore (crash simulator)
// --------------------------------------------------------------------------

/// How a crash that lands on a `flush` interacts with the writes that flush was
/// about to make durable.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tear {
    /// The flush did not happen: nothing written since the previous successful
    /// flush reaches durable storage. Models a crash that interrupts `fsync`
    /// before any dirty page is written back.
    Clean,
    /// The flush partially happened: every write since the previous flush
    /// reaches durable storage *except* the most recent one, which is torn to
    /// its first `prefix` bytes (the tail keeps its prior durable value). Models
    /// a torn write of the in-flight region — the case the superblock CRC must
    /// catch.
    TornLastWrite { prefix: usize },
}

/// The crash configuration for one fuzzer run.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CrashPoint {
    /// Crash after this many successful syscalls (writes + flushes). A value at
    /// or beyond the commit's total syscall count means "no crash".
    pub after_syscalls: u32,
    /// How a crash landing on a flush tears.
    pub tear: Tear,
}

impl CrashPoint {
    /// A configuration that never crashes (the commit runs to completion).
    pub fn never() -> Self {
        CrashPoint {
            after_syscalls: u32::MAX,
            tear: Tear::Clean,
        }
    }
}

/// A crash-simulating block store. It separates *live* bytes (the page cache,
/// where writes land immediately) from *durable* bytes (what survives a crash,
/// updated only by a successful flush). A configured [`CrashPoint`] makes the
/// store "crash" after a chosen number of syscalls: the crashing syscall and
/// every later one returns an error, and on recovery the bundle is reopened from
/// [`FaultStore::durable_image`].
pub struct FaultStore {
    durable: Vec<u8>,
    live: Vec<u8>,
    /// Writes (offset, len) since the last successful flush, in order.
    pending: Vec<(u64, usize)>,
    /// Syscalls performed so far (writes + flushes).
    issued: u32,
    /// Crash configuration.
    crash: CrashPoint,
    /// Set once the store has crashed; all further syscalls fail.
    crashed: bool,
}

impl FaultStore {
    /// A fault store seeded with a durable image, configured to crash at
    /// `crash`. `live` starts equal to `durable` (a freshly opened file's page
    /// cache matches the disk).
    pub fn new(durable_image: Vec<u8>, crash: CrashPoint) -> Self {
        FaultStore {
            live: durable_image.clone(),
            durable: durable_image,
            pending: Vec::new(),
            issued: 0,
            crash,
            crashed: false,
        }
    }

    /// A fault store that never crashes (used to run a commit fully and learn
    /// its total syscall count and post-commit image).
    pub fn no_fault(durable_image: Vec<u8>) -> Self {
        Self::new(durable_image, CrashPoint::never())
    }

    /// The number of syscalls performed (meaningful after a no-fault run: the
    /// commit's total syscall count, i.e. the exhaustive crash-point bound).
    pub fn syscalls_issued(&self) -> u32 {
        self.issued
    }

    /// Whether the store has crashed.
    pub fn crashed(&self) -> bool {
        self.crashed
    }

    /// The durable image: the bytes that survive a crash at the configured
    /// point. Recovery opens a fresh store over this.
    pub fn durable_image(&self) -> Vec<u8> {
        self.durable.clone()
    }

    /// The current durable image as a [`MemStore`], ready to reopen.
    pub fn recover(&self) -> MemStore {
        MemStore::from_bytes(self.durable_image())
    }

    fn simulated_crash() -> io::Error {
        io::Error::other("simulated crash")
    }

    /// Promotes the most recent pending write torn to `prefix` bytes, and all
    /// earlier pending writes fully. `live` already holds every write; we copy
    /// it into `durable`, then revert the torn tail of the last write to its
    /// prior durable value (or zero, if that region did not exist before).
    fn apply_torn_flush(&mut self, prefix: usize) {
        if let Some(&(off, len)) = self.pending.last() {
            // Capture the prior durable tail before we overwrite `durable`.
            let mut prior_tail = vec![0u8; len];
            for (i, slot) in prior_tail.iter_mut().enumerate() {
                if let Some(b) = self.durable.get(off as usize + i) {
                    *slot = *b;
                }
            }
            self.durable = self.live.clone();
            let torn_from = (off as usize) + prefix.min(len);
            let region_end = off as usize + len;
            for (i, idx) in (torn_from..region_end).enumerate() {
                self.durable[idx] = prior_tail[prefix.min(len) + i];
            }
        } else {
            // Nothing pending: a no-op flush.
            self.durable = self.live.clone();
        }
    }
}

impl BlockStore for FaultStore {
    fn len(&self) -> u64 {
        self.live.len() as u64
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        // Reads see live bytes (the writing process's own view). Reads are not
        // crash points: the commit path issues only writes and flushes.
        let end = offset as usize + buf.len();
        if end > self.live.len() {
            return Err(out_of_range());
        }
        buf.copy_from_slice(&self.live[offset as usize..end]);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        if self.crashed {
            return Err(Self::simulated_crash());
        }
        if self.issued >= self.crash.after_syscalls {
            // The crash lands on a write: it does not reach durable storage
            // (it was never flushed), so durable is simply left as-is.
            self.crashed = true;
            return Err(Self::simulated_crash());
        }
        self.issued += 1;
        apply_write(&mut self.live, offset, data);
        self.pending.push((offset, data.len()));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.crashed {
            return Err(Self::simulated_crash());
        }
        if self.issued >= self.crash.after_syscalls {
            // The crash lands on this flush; it persists per the tear mode.
            self.crashed = true;
            match self.crash.tear {
                Tear::Clean => { /* durable unchanged */ }
                Tear::TornLastWrite { prefix } => self.apply_torn_flush(prefix),
            }
            return Err(Self::simulated_crash());
        }
        self.issued += 1;
        // A successful flush makes everything durable.
        self.durable = self.live.clone();
        self.pending.clear();
        Ok(())
    }
}

// --------------------------------------------------------------------------
// PageCacheStore (a failed fsync, lived through)
// --------------------------------------------------------------------------

/// What a failed flush leaves of the writes it was making durable, as Linux's
/// filesystems behave (Rebello et al., "Can Applications Recover from fsync
/// Failures?", USENIX ATC 2020, §4, on Linux 5.2.11: "All the file systems mark
/// the page clean even after fsync fails ... In both modes of ext4 and XFS, the
/// page contains the latest write while Btrfs reverts the in-memory state to be
/// consistent with what is on disk").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FailedFlush {
    /// ext4 and XFS: the new bytes stay readable, but their pages are marked
    /// clean, so the disk keeps the old bytes and no later flush writes the new
    /// ones unless they are written again ("future calls to fsync never retry
    /// previous data writes that may have failed").
    MarksClean,
    /// Btrfs: what a read sees reverts to what is on disk.
    Reverts,
}

/// A page cache over a disk whose chosen flushes fail, for a writer that lives
/// on after the failure, reads the store and flushes again. Reads see the
/// *live* bytes (the page cache); a successful flush writes to the *durable*
/// bytes (the disk) every unit written since the previous flush, whole, and the
/// file's length; a failed flush writes none of them and leaves them as
/// [`FailedFlush`] says. The unit is a byte unless [`PageCacheStore::with_unit`]
/// sets another, such as the 4,096-byte page ext4 and XFS write back. A flush
/// with nothing written since sends nothing to the disk, so it succeeds even
/// where it was chosen to fail: a flush retried after a failure reports success
/// without writing what failed. A clean unit can be evicted before a chosen
/// read ([`PageCacheStore::evicting_before`]) and is then read from the disk.
///
/// [`FaultStore`] models a crash, after which recovery reads only the disk; this
/// models the failure a reconciling writer must not mistake for durability
/// (`Bundle::sync`).
pub struct PageCacheStore {
    /// Interior, so a read can evict.
    live: RefCell<Vec<u8>>,
    durable: Vec<u8>,
    /// Writes (offset, len) since the last flush, failed or not.
    dirty: Vec<(u64, usize)>,
    unit: usize,
    flushes: u32,
    failing: Vec<u32>,
    on_failure: FailedFlush,
    evict_before: Vec<u32>,
    /// Reads since the first failed flush, from none until one fails.
    reads_since_failure: Cell<Option<u32>>,
    evictions: Cell<u32>,
}

impl PageCacheStore {
    /// A store over a durable `image`, read as it is on disk, whose flushes
    /// numbered in `failing` (the first flush is 1) fail as `on_failure` says.
    pub fn new(image: Vec<u8>, failing: Vec<u32>, on_failure: FailedFlush) -> Self {
        PageCacheStore {
            live: RefCell::new(image.clone()),
            durable: image,
            dirty: Vec::new(),
            unit: 1,
            flushes: 0,
            failing,
            on_failure,
            evict_before: Vec::new(),
            reads_since_failure: Cell::new(None),
            evictions: Cell::new(0),
        }
    }

    /// The store tracking writes by `unit` bytes, the unit it writes back and
    /// evicts: a write dirties every unit it touches. A byte (the default) is
    /// stricter than a filesystem, which writes back a page or a block whole.
    pub fn with_unit(self, unit: usize) -> Self {
        assert!(unit > 0, "a unit holds at least a byte");
        PageCacheStore { unit, ..self }
    }

    /// The store evicting every clean unit before each read numbered in
    /// `reads`, counted from the first failed flush (the first read after it
    /// is 1): the unit is read again from the disk, as a page a failed `fsync`
    /// marked clean holds the new bytes "only until the page has been evicted
    /// from the page cache" (Rebello et al., §4). A unit written since the last
    /// flush is dirty, and stays.
    pub fn evicting_before(self, reads: Vec<u32>) -> Self {
        PageCacheStore {
            evict_before: reads,
            ..self
        }
    }

    /// What a read sees now: the page cache.
    pub fn live(&self) -> Vec<u8> {
        self.live.borrow().clone()
    }

    /// What is on disk: what a crash, or a page evicted and read again, leaves.
    pub fn durable(&self) -> &[u8] {
        &self.durable
    }

    /// The flushes run so far, failed ones included.
    pub fn flushes(&self) -> u32 {
        self.flushes
    }

    /// The evictions run so far.
    pub fn evictions(&self) -> u32 {
        self.evictions.get()
    }

    /// The units written since the last flush, as byte ranges a unit wide.
    fn dirty_units(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.dirty
            .iter()
            .filter(|&&(_, len)| len > 0)
            .map(|&(offset, len)| {
                let start = offset as usize / self.unit * self.unit;
                let end = (offset as usize + len).div_ceil(self.unit) * self.unit;
                (start, end)
            })
    }

    /// Every clean unit within the disk's length read again from the disk.
    fn evict(&self) {
        let mut live = self.live.borrow_mut();
        let len = self.durable.len().min(live.len());
        for start in (0..len).step_by(self.unit) {
            let end = (start + self.unit).min(len);
            if !self.dirty_units().any(|(s, e)| s < end && start < e) {
                live[start..end].copy_from_slice(&self.durable[start..end]);
            }
        }
        self.evictions.set(self.evictions.get() + 1);
    }
}

impl BlockStore for PageCacheStore {
    fn len(&self) -> u64 {
        self.live.borrow().len() as u64
    }

    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        if let Some(reads) = self.reads_since_failure.get() {
            self.reads_since_failure.set(Some(reads + 1));
            if self.evict_before.contains(&(reads + 1)) {
                self.evict();
            }
        }
        let live = self.live.borrow();
        let end = offset as usize + buf.len();
        if end > live.len() {
            return Err(out_of_range());
        }
        buf.copy_from_slice(&live[offset as usize..end]);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> io::Result<()> {
        apply_write(self.live.get_mut(), offset, data);
        self.dirty.push((offset, data.len()));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.dirty.is_empty() {
            return Ok(());
        }
        if self.failing.contains(&self.flushes) {
            // Marked clean, the writes not on the disk.
            self.dirty.clear();
            if self.reads_since_failure.get().is_none() {
                self.reads_since_failure.set(Some(0));
            }
            if self.on_failure == FailedFlush::Reverts {
                *self.live.get_mut() = self.durable.clone();
            }
            return Err(io::Error::other("flush failed: input/output error"));
        }
        let units: Vec<_> = self.dirty_units().collect();
        self.dirty.clear();
        let live = self.live.get_mut();
        for (start, end) in units {
            let end = end.min(live.len());
            if start < end {
                apply_write(&mut self.durable, start as u64, &live[start..end]);
            }
        }
        // The length, as `fsync` writes the file's size: bytes past the old end
        // that no successful flush wrote read from disk as zeros.
        self.durable.resize(live.len(), 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memstore_reads_writes_and_extends() {
        let mut s = MemStore::new();
        s.write_at(4, b"abcd").unwrap();
        assert_eq!(s.len(), 8);
        let mut buf = [0u8; 4];
        s.read_exact_at(4, &mut buf).unwrap();
        assert_eq!(&buf, b"abcd");
        // The gap is zero-filled.
        let mut head = [0xFFu8; 4];
        s.read_exact_at(0, &mut head).unwrap();
        assert_eq!(head, [0, 0, 0, 0]);
    }

    #[test]
    fn memstore_read_past_end_errors() {
        let s = MemStore::from_bytes(vec![1, 2, 3]);
        let mut buf = [0u8; 4];
        assert!(s.read_exact_at(0, &mut buf).is_err());
    }

    #[test]
    fn fault_store_unflushed_writes_are_not_durable() {
        let mut s = FaultStore::new(Vec::new(), CrashPoint::never());
        s.write_at(0, b"hello").unwrap();
        // No flush yet: durable is still empty.
        assert!(s.durable_image().is_empty());
        s.flush().unwrap();
        assert_eq!(s.durable_image(), b"hello");
    }

    #[test]
    fn fault_store_crashes_on_the_configured_syscall() {
        // Allow 1 syscall (the write), crash on the 2nd (the flush), clean.
        let mut s = FaultStore::new(
            Vec::new(),
            CrashPoint {
                after_syscalls: 1,
                tear: Tear::Clean,
            },
        );
        s.write_at(0, b"xyz").unwrap();
        assert!(s.flush().is_err(), "flush is the crashing syscall");
        assert!(s.crashed());
        // Clean crash on the flush: the unflushed write is lost.
        assert!(s.durable_image().is_empty());
    }

    #[test]
    fn fault_store_torn_flush_persists_a_prefix() {
        let mut s = FaultStore::new(
            vec![0xAA; 8],
            CrashPoint {
                after_syscalls: 1,
                tear: Tear::TornLastWrite { prefix: 3 },
            },
        );
        // Overwrite all 8 bytes with 0xBB, but the flush tears after 3 bytes.
        s.write_at(0, &[0xBB; 8]).unwrap();
        assert!(s.flush().is_err());
        let durable = s.durable_image();
        assert_eq!(&durable[0..3], &[0xBB, 0xBB, 0xBB], "prefix persisted");
        assert_eq!(&durable[3..8], &[0xAA; 5], "tail kept its prior value");
    }

    #[test]
    fn fault_store_counts_syscalls() {
        let mut s = FaultStore::no_fault(Vec::new());
        s.write_at(0, b"a").unwrap();
        s.write_at(1, b"b").unwrap();
        s.flush().unwrap();
        assert_eq!(s.syscalls_issued(), 3);
    }

    #[test]
    fn page_cache_store_marking_clean_keeps_the_write_off_the_disk_until_it_is_written_again() {
        let mut s = PageCacheStore::new(vec![0xAA; 8], vec![1], FailedFlush::MarksClean);
        s.write_at(0, &[0xBB; 4]).unwrap();
        assert!(s.flush().is_err());
        assert_eq!(&s.live()[..4], &[0xBB; 4], "the write stays readable");
        assert_eq!(s.durable(), &[0xAA; 8], "and off the disk");
        s.flush().expect("nothing dirty: a retried flush succeeds");
        assert_eq!(s.durable(), &[0xAA; 8], "without writing it");
        s.write_at(4, &[0xCC; 4]).unwrap();
        s.flush().unwrap();
        assert_eq!(&s.durable()[..4], &[0xAA; 4], "only what was written since");
        assert_eq!(&s.durable()[4..], &[0xCC; 4]);
        s.write_at(0, &[0xBB; 4]).unwrap();
        s.flush().unwrap();
        assert_eq!(s.durable(), s.live(), "written again, it reaches the disk");
        assert_eq!(s.flushes(), 4);
    }

    #[test]
    fn page_cache_store_reverting_reads_the_disk_again() {
        let mut s = PageCacheStore::new(vec![0xAA; 4], vec![1], FailedFlush::Reverts);
        s.write_at(2, &[0xBB; 4]).unwrap();
        assert!(s.flush().is_err());
        assert_eq!(s.live(), &[0xAA; 4]);
        assert_eq!(s.durable(), &[0xAA; 4]);
    }

    #[test]
    fn page_cache_store_writes_the_length_with_a_successful_flush() {
        let mut s = PageCacheStore::new(Vec::new(), vec![1], FailedFlush::MarksClean);
        s.write_at(0, b"abcd").unwrap();
        assert!(s.flush().is_err());
        s.write_at(4, b"ef").unwrap();
        s.flush().unwrap();
        assert_eq!(
            s.durable(),
            b"\0\0\0\0ef",
            "the failed range reads as zeros"
        );
        assert_eq!(s.live(), b"abcdef");
    }

    #[test]
    fn page_cache_store_writes_back_whole_units() {
        for (unit, healed) in [(1, false), (4, true)] {
            let mut s = PageCacheStore::new(vec![0xAA; 8], vec![1], FailedFlush::MarksClean)
                .with_unit(unit);
            s.write_at(1, &[0xBB]).unwrap();
            assert!(s.flush().is_err());
            s.write_at(2, &[0xCC]).unwrap();
            s.flush().unwrap();
            assert_eq!(s.durable()[2], 0xCC);
            assert_eq!(
                s.durable()[1] == 0xBB,
                healed,
                "unit {unit}: the failed byte is written back with its unit"
            );
            assert_eq!(&s.durable()[4..], &[0xAA; 4], "an unwritten unit is not");
        }
    }

    #[test]
    fn page_cache_store_evicts_clean_units_before_the_chosen_reads() {
        let mut s = PageCacheStore::new(vec![0xAA; 8], vec![1], FailedFlush::MarksClean)
            .with_unit(4)
            .evicting_before(vec![2]);
        let mut byte = [0u8];
        s.read_exact_at(1, &mut byte).unwrap();
        s.write_at(1, &[0xBB]).unwrap();
        assert!(s.flush().is_err());
        s.read_exact_at(1, &mut byte).unwrap();
        assert_eq!(byte, [0xBB], "read 1 after the failure: still readable");
        s.write_at(6, &[0xCC]).unwrap();
        s.read_exact_at(1, &mut byte).unwrap();
        assert_eq!(byte, [0xAA], "read 2: evicted, it is read from the disk");
        assert_eq!(s.live()[6], 0xCC, "a dirty unit stays");
        assert_eq!(s.evictions(), 1);
        s.read_exact_at(1, &mut byte).unwrap();
        assert_eq!(s.evictions(), 1, "only before the chosen reads");
    }
}
