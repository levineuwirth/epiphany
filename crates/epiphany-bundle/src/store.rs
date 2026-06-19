//! The block-store abstraction and its implementations.
//!
//! The bundle is a single file accessed at byte offsets with an explicit
//! *durable flush* at the commit point (Chapter 8 §"Durable Writes": the flush
//! is `fsync`/`FlushFileBuffers`/equivalent — *"durability MUST be requested
//! explicitly at the commit point"*; implementations *"MAY NOT rely on
//! filesystem ordering guarantees or write-back caching alone"*).
//!
//! [`BlockStore`] captures exactly that contract: positioned reads/writes plus a
//! `flush` that makes prior writes durable. Three implementations:
//!
//! * [`MemStore`] — a plain in-memory image (`flush` is a no-op; the bytes *are*
//!   durable). The default for tests and for the cold-open path over an image.
//! * [`FileStore`] — a real file whose `flush` calls `fsync`. Demonstrates the
//!   atomic-commit protocol against a real filesystem.
//! * [`FaultStore`] — a crash simulator that distinguishes *live* (page-cache)
//!   bytes from *durable* (survives-a-crash) bytes and can "crash" after any
//!   syscall, optionally tearing the in-flight write. This is the engine behind
//!   the crash-recovery fuzzer — Agent D's acceptance gate.

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
}
