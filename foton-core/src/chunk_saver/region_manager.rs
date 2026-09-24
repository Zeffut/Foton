//! Region file manager with seek-based chunk access.
//!
//! Uses a sector-based format where only the header (8KB) is kept in memory.
//! Chunk data is read on-demand from disk and converted directly to runtime
//! format, avoiding memory duplication.

#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    fmt,
    io::{self},
    mem,
    path::{Path, PathBuf},
    sync::Weak,
};
use zstd::bulk::decompress;

#[cfg(test)]
use foton_utils::locks::SyncMutex;
use foton_utils::{ChunkPos, locks::AsyncRwLock};
use rustc_hash::FxHashMap;
#[cfg(test)]
use tokio::sync::Notify;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::oneshot,
};

use crate::chunk::status::ChunkStatus;
use crate::world::World;

use super::{
    ChunkStorage, LoadedChunk, PersistentChunk,
    format::{
        CHUNK_TABLE_SIZE, ChunkEntry, FILE_HEADER_SIZE, FIRST_DATA_SECTOR, FORMAT_VERSION,
        MAX_CHUNK_SIZE, REGION_MAGIC, RegionHeader, RegionPos, SECTOR_SIZE,
    },
};

#[derive(Debug)]
struct CorruptChunkData(String);

impl fmt::Display for CorruptChunkData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// How many corrupt copies one chunk slot may accumulate before the oldest ones
/// are simply kept and no further copy is written. Bounds the disk cost of a
/// slot that goes corrupt, regenerates, and goes corrupt again.
const MAX_QUARANTINED_COPIES_PER_SLOT: u32 = 16;

/// Largest file whose sector count and chunk offsets fit the region layout.
///
/// Saves append new chunk data without reclaiming old sectors, so the bound
/// must include every sector addressable by the on-disk `u32` offsets rather
/// than only the sectors currently retained by the chunk table.
const MAX_REGION_FILE_SIZE: u64 = u32::MAX as u64 * SECTOR_SIZE as u64;

/// Manages region files with seek-based chunk access.
///
/// Only keeps region headers (8KB each) in memory, not chunk data.
/// Chunks are loaded on-demand and converted directly to runtime format.
pub struct RegionManager {
    /// Base directory for region files (e.g., "world/region").
    base_path: PathBuf,
    /// Open region file handles with their headers.
    regions: AsyncRwLock<FxHashMap<RegionPos, RegionHandle>>,
    #[cfg(test)]
    directory_sync_failure: SyncMutex<Option<PathBuf>>,
    #[cfg(test)]
    directory_syncs: SyncMutex<Vec<PathBuf>>,
    #[cfg(test)]
    header_write_failures: AtomicUsize,
    #[cfg(test)]
    pause_header_writes: AtomicBool,
    #[cfg(test)]
    header_write_started: Notify,
    #[cfg(test)]
    resume_header_writes: Notify,
}

/// Prepared chunk data ready to be saved asynchronously.
/// Created by `prepare_chunk_save` during the holder's snapshot-preparation phase.
pub struct PreparedChunkSave {
    /// The chunk position.
    pub pos: ChunkPos,
    /// The highest persisted status captured with the chunk data.
    pub status: ChunkStatus,
    /// The serialized chunk data.
    pub persistent: PersistentChunk<'static>,
    /// Runtime manager entity IDs that were either serialized or explicitly skipped.
    pub handled_runtime_entity_ids: Vec<i32>,
}

/// An open region file with its header.
struct RegionHandle {
    /// File handle for reading/writing.
    file: File,
    /// Chunk location header (8KB).
    header: RegionHeader,
    /// Number of chunks currently loaded from this region.
    loaded_chunk_count: usize,
    /// Whether the header has been modified since last save.
    header_dirty: bool,
    /// Current file size in sectors.
    file_sectors: u32,
}

/// The most a single chunk may expand to when it is decompressed.
///
/// Chunks run to a few hundred kilobytes in practice, and the region format
/// caps one at 255 sectors -- about a megabyte -- on disk. Sixty-four megabytes
/// is far past anything legitimate while still turning a decompression bomb
/// into a rejected chunk rather than an out-of-memory kill.
const MAX_DECOMPRESSED_CHUNK_BYTES: usize = 64 * 1024 * 1024;

impl RegionManager {
    /// Creates a new region manager.
    ///
    /// # Arguments
    /// * `base_path` - Directory where region files are stored.
    /// * `registry` - The registry for block state and biome conversions.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            regions: AsyncRwLock::new(FxHashMap::default()),
            #[cfg(test)]
            directory_sync_failure: SyncMutex::new(None),
            #[cfg(test)]
            directory_syncs: SyncMutex::new(Vec::new()),
            #[cfg(test)]
            header_write_failures: AtomicUsize::new(0),
            #[cfg(test)]
            pause_header_writes: AtomicBool::new(false),
            #[cfg(test)]
            header_write_started: Notify::new(),
            #[cfg(test)]
            resume_header_writes: Notify::new(),
        }
    }

    /// Gets the file path for a region.
    fn region_path(&self, pos: RegionPos) -> PathBuf {
        self.base_path.join(pos.filename())
    }

    /// Opens or creates a region file, loading only the header.
    async fn open_region(&self, pos: RegionPos) -> io::Result<RegionHandle> {
        let path = self.region_path(pos);

        if !path.exists() {
            // Create new region file with empty header
            return self.create_region(pos).await;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await?;

        let file_size = file.metadata().await?.len();
        // Read and verify magic + version
        let mut header_bytes = [0u8; FILE_HEADER_SIZE];
        file.read_exact(&mut header_bytes).await?;

        let magic = &header_bytes[0..4];
        if magic != REGION_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid region file magic",
            ));
        }

        let version = u16::from_le_bytes([header_bytes[4], header_bytes[5]]);
        if version != FORMAT_VERSION {
            // Version mismatch — backup the old file and create a fresh region.
            drop(file);
            let backup_path = self.backup_incompatible_region(&path, version).await?;
            tracing::warn!(
                "Region file {} has version {version} (expected {FORMAT_VERSION}), backing up to {} and recreating",
                path.display(),
                backup_path.display()
            );
            return self.create_region(pos).await;
        }

        if file_size > MAX_REGION_FILE_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "region file is {file_size} bytes, exceeding the format limit of {MAX_REGION_FILE_SIZE} bytes"
                ),
            ));
        }

        // Read chunk table
        let mut table_bytes = vec![0u8; CHUNK_TABLE_SIZE];
        file.read_exact(&mut table_bytes).await?;
        let header = RegionHeader::from_bytes(&table_bytes).map_err(|index| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("region chunk table entry {index} has an invalid status byte"),
            )
        })?;

        // Calculate file size in sectors
        let file_sectors = file_size.div_ceil(SECTOR_SIZE as u64) as u32;
        Self::validate_region_entries(&header, file_sectors)?;

        Ok(RegionHandle {
            file,
            header,
            loaded_chunk_count: 0,
            header_dirty: false,
            file_sectors,
        })
    }

    /// Moves an incompatible region aside without replacing an earlier backup.
    async fn backup_incompatible_region(
        &self,
        path: &PathBuf,
        version: u16,
    ) -> io::Result<PathBuf> {
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "region path has no parent directory",
            )
        })?;
        let extension = format!("srg.v{version}.bak");
        for attempt in 0..=u32::MAX {
            let backup_path = if attempt == 0 {
                path.with_extension(&extension)
            } else {
                path.with_extension(format!("{extension}.{attempt}"))
            };

            match fs::hard_link(path, &backup_path).await {
                Ok(()) => {
                    self.sync_directory(parent).await?;
                    if let Err(remove_error) = fs::remove_file(path).await {
                        return match fs::remove_file(&backup_path).await {
                            Ok(()) => match self.sync_directory(parent).await {
                                Ok(()) => Err(remove_error),
                                Err(sync_error) => Err(io::Error::new(
                                    sync_error.kind(),
                                    format!(
                                        "failed to remove incompatible region ({remove_error}); removed backup cleanup but failed to sync its directory ({sync_error})"
                                    ),
                                )),
                            },
                            Err(cleanup_error) => Err(io::Error::new(
                                cleanup_error.kind(),
                                format!(
                                    "failed to remove incompatible region after linking its backup ({remove_error}); failed to clean up {} ({cleanup_error})",
                                    backup_path.display()
                                ),
                            )),
                        };
                    }
                    self.sync_directory(parent).await?;
                    return Ok(backup_path);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "all incompatible-region backup suffixes are occupied",
        ))
    }

    /// Creates a new empty region file.
    async fn create_region(&self, pos: RegionPos) -> io::Result<RegionHandle> {
        self.create_directories_durable(&self.base_path).await?;

        let path = self.region_path(pos);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .await?;

        // Write header
        let mut header_bytes = [0u8; FILE_HEADER_SIZE];
        header_bytes[0..4].copy_from_slice(&REGION_MAGIC);
        header_bytes[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        file.write_all(&header_bytes).await?;

        // Write empty chunk table
        let header = RegionHeader::new();
        file.write_all(&header.to_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        self.sync_directory(&self.base_path).await?;

        Ok(RegionHandle {
            file,
            header,
            loaded_chunk_count: 0,
            header_dirty: false,
            file_sectors: FIRST_DATA_SECTOR,
        })
    }

    /// Writes the header to disk.
    ///
    /// This is the commit point of a chunk save: until the header names the new
    /// sectors, the old copy is what a reload finds. It is fsynced for the same
    /// reason a database fsyncs its commit record.
    async fn write_header(&self, file: &mut File, header: &RegionHeader) -> io::Result<()> {
        #[cfg(test)]
        if self.pause_header_writes.load(Ordering::Acquire) {
            self.header_write_started.notify_one();
            self.resume_header_writes.notified().await;
        }
        #[cfg(test)]
        if self
            .header_write_failures
            .try_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            return Err(io::Error::other("injected header write failure"));
        }
        file.seek(io::SeekFrom::Start(FILE_HEADER_SIZE as u64))
            .await?;
        file.write_all(&header.to_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }

    /// Reads a chunk's compressed data from disk.
    async fn read_chunk_data(
        file: &mut File,
        sector_offset: u32,
        size: u32,
    ) -> io::Result<Vec<u8>> {
        let byte_offset = u64::from(sector_offset) * SECTOR_SIZE as u64;
        file.seek(io::SeekFrom::Start(byte_offset)).await?;

        let mut compressed = vec![0u8; size as usize];
        file.read_exact(&mut compressed).await?;
        Ok(compressed)
    }

    fn validate_chunk_entry(entry: ChunkEntry, file_sectors: u32) -> io::Result<()> {
        if entry.sector_offset < FIRST_DATA_SECTOR {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "chunk table entry points into the region header at sector {}",
                    entry.sector_offset
                ),
            ));
        }
        if entry.size_bytes == 0 || entry.size_bytes as usize > MAX_CHUNK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid chunk table entry size {}", entry.size_bytes),
            ));
        }
        let Some(end_sector) = entry.sector_offset.checked_add(entry.sector_count()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk table entry sector range overflowed",
            ));
        };
        if end_sector > file_sectors {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "chunk table entry ends at sector {end_sector}, past region end {file_sectors}"
                ),
            ));
        }
        Ok(())
    }

    fn validate_region_entries(header: &RegionHeader, file_sectors: u32) -> io::Result<()> {
        let mut allocations = Vec::with_capacity(CHUNK_TABLE_SIZE / 8);
        for (index, &entry) in header.entries.iter().enumerate() {
            if !entry.exists() {
                continue;
            }
            Self::validate_chunk_entry(entry, file_sectors)?;
            allocations.push((
                entry.sector_offset,
                entry.sector_offset + entry.sector_count(),
                index,
            ));
        }
        allocations.sort_unstable_by_key(|&(start, _, _)| start);
        for pair in allocations.windows(2) {
            let (_, previous_end, _) = pair[0];
            let (current_start, _, current_index) = pair[1];
            if current_start < previous_end {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("chunk table entry {current_index} overlaps another region allocation"),
                ));
            }
        }
        Ok(())
    }

    async fn create_directories_durable(&self, directory: &Path) -> io::Result<()> {
        let mut missing = Vec::new();
        let mut current = directory.to_owned();
        while !fs::try_exists(&current).await? {
            let parent = current.parent().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("directory {} has no parent", current.display()),
                )
            })?;
            let parent = if parent.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                parent.to_owned()
            };
            missing.push(current);
            current = parent;
        }

        while let Some(path) = missing.pop() {
            match fs::create_dir(&path).await {
                Ok(()) => {
                    let parent = path.parent().ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("directory {} has no parent", path.display()),
                        )
                    })?;
                    let parent = if parent.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        parent
                    };
                    self.sync_directory(parent).await?;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        #[cfg(test)]
        {
            self.directory_syncs.lock().push(directory.to_owned());
            if self.directory_sync_failure.lock().as_deref() == Some(directory) {
                return Err(io::Error::other("injected directory sync failure"));
            }
        }
        #[cfg(unix)]
        {
            File::open(directory).await?.sync_all().await?;
        }
        #[cfg(not(unix))]
        let _ = directory;
        Ok(())
    }

    /// Where a chunk's bytes are put before its slot is cleared.
    ///
    /// Never returns a path that already holds a copy. A cleared slot is
    /// refilled by worldgen, so the *second* time an index goes corrupt the
    /// bytes are regenerated terrain -- overwriting the first copy would trade
    /// the player's build for a hillside. The first copy wins, and later ones
    /// queue up beside it.
    async fn free_quarantine_path(
        &self,
        region_pos: RegionPos,
        index: usize,
    ) -> io::Result<PathBuf> {
        let directory = self.base_path.join("corrupt");
        let stem = format!("{}.{index}", region_pos.filename());
        for attempt in 0..MAX_QUARANTINED_COPIES_PER_SLOT {
            let path = if attempt == 0 {
                directory.join(format!("{stem}.chunk"))
            } else {
                directory.join(format!("{stem}.{attempt}.chunk"))
            };
            if !fs::try_exists(&path).await? {
                return Ok(path);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("chunk slot already has {MAX_QUARANTINED_COPIES_PER_SLOT} quarantined copies"),
        ))
    }

    /// Copies a chunk's raw bytes aside so clearing its slot destroys nothing.
    ///
    async fn quarantine_chunk_bytes(
        &self,
        file: &mut File,
        region_pos: RegionPos,
        index: usize,
        entry: ChunkEntry,
    ) -> io::Result<()> {
        let path = self.free_quarantine_path(region_pos, index).await?;
        let bytes = Self::read_chunk_data(file, entry.sector_offset, entry.size_bytes).await?;
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "quarantine path has no parent directory",
            )
        })?;
        self.create_directories_durable(parent).await?;
        let mut quarantine = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await?;
        if let Err(write_error) = quarantine.write_all(&bytes).await {
            drop(quarantine);
            return match fs::remove_file(&path).await {
                Ok(()) => Err(write_error),
                Err(cleanup_error) => Err(io::Error::new(
                    cleanup_error.kind(),
                    format!(
                        "failed to write quarantine copy ({write_error}); failed to clean up {} ({cleanup_error})",
                        path.display()
                    ),
                )),
            };
        }
        quarantine.flush().await?;
        quarantine.sync_all().await?;
        drop(quarantine);
        self.sync_directory(parent).await?;
        tracing::warn!(
            path = %path.display(),
            bytes = bytes.len(),
            "Chunk failed to decode; its bytes were copied aside before the slot was cleared"
        );
        Ok(())
    }

    fn set_pending_header_entry(handle: &mut RegionHandle, index: usize, entry: ChunkEntry) {
        handle.header.entries[index] = entry;
        handle.header_dirty = true;
    }

    async fn clear_corrupt_chunk_if_unchanged(
        &self,
        region_pos: RegionPos,
        index: usize,
        expected_entry: ChunkEntry,
    ) -> io::Result<bool> {
        let mut regions = self.regions.write().await;
        let Some(handle) = regions.get_mut(&region_pos) else {
            return Err(io::Error::other(
                "region was released while clearing corrupt chunk data",
            ));
        };
        if handle.header.entries[index] != expected_entry {
            return Ok(false);
        }

        // Clearing the slot lets worldgen refill the column, which is how the
        // server stays usable -- but the bytes it replaces are the only copy of
        // whatever a player built there. Keep them.
        self.quarantine_chunk_bytes(&mut handle.file, region_pos, index, expected_entry)
            .await?;

        Self::set_pending_header_entry(handle, index, ChunkEntry::empty());
        self.write_header(&mut handle.file, &handle.header).await?;
        handle.header_dirty = false;
        Ok(true)
    }

    /// Writes chunk data to disk at the specified sector offset.
    async fn write_chunk_data(
        file: &mut File,
        sector_offset: u32,
        data: &[u8],
        file_sectors: &mut u32,
    ) -> io::Result<()> {
        let sectors_used = u32::try_from(data.len().div_ceil(SECTOR_SIZE)).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "chunk data sector count exceeds the region format",
            )
        })?;
        let end_sector = sector_offset.checked_add(sectors_used).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "chunk data sector range exceeds the region format",
            )
        })?;
        let byte_offset = u64::from(sector_offset) * SECTOR_SIZE as u64;
        file.seek(io::SeekFrom::Start(byte_offset)).await?;
        file.write_all(data).await?;

        // Pad to sector boundary
        let padding_needed = (SECTOR_SIZE - (data.len() % SECTOR_SIZE)) % SECTOR_SIZE;
        if padding_needed > 0 {
            file.write_all(&vec![0u8; padding_needed]).await?;
        }

        // Update file sectors if we wrote past the end
        if end_sector > *file_sectors {
            *file_sectors = end_sector;
        }

        file.flush().await?;
        // `flush` only pushes the userspace buffer into the kernel. The header
        // that will point at these sectors is fsynced separately, and the two
        // orderings only mean anything if the data is durable first -- so this
        // one is a real fsync.
        file.sync_all().await?;
        Ok(())
    }

    /// Saves prepared chunk data to disk after the snapshot-preparation phase has ended.
    #[expect(
        clippy::missing_panics_doc,
        reason = "panic on `just inserted` is unreachable"
    )]
    pub async fn save_chunk_data(
        &self,
        prepared: PreparedChunkSave,
        thread_pool: &rayon::ThreadPool,
    ) -> io::Result<bool> {
        let pos = prepared.pos;
        let status = prepared.status;
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);

        let (sender, receiver) = oneshot::channel();
        thread_pool.spawn(move || {
            let result = Self::encode_chunk(prepared);
            if sender.send(result).is_err() {
                tracing::trace!(
                    chunk = ?pos,
                    "Discarding encoded chunk after its save task was canceled"
                );
            }
        });
        let compressed = receiver.await.map_err(|_| {
            io::Error::other("chunk encode task ended without returning a result")
        })??;

        let mut regions = self.regions.write().await;

        // Track if we opened the region (so we can close it after)
        let we_opened_region = !regions.contains_key(&region_pos);

        // Get or open the region
        let handle = if let Some(handle) = regions.get_mut(&region_pos) {
            handle
        } else {
            let handle = self.open_region(region_pos).await?;
            regions.insert(region_pos, handle);
            #[cfg_attr(
                not(test),
                expect(
                    clippy::expect_used,
                    reason = "inserted on the line above; the entry API cannot be used here because opening a region awaits"
                )
            )]
            let handle = regions.get_mut(&region_pos).expect("just inserted");
            handle
        };

        // Find space for the chunk.
        //
        // Never reuse the sectors the chunk already occupies, even when the new
        // copy would fit. Overwriting in place destroys the only valid copy the
        // moment the write starts: a crash, a full disk or an OOM kill halfway
        // through leaves neither the old chunk nor a complete new one, the
        // decode fails on the next load, and `clear_corrupt_chunk_if_unchanged`
        // then wipes the slot so the world silently regenerates the terrain --
        // taking whatever the player built there with it.
        //
        // Writing to free sectors instead makes the header update the commit
        // point: until it lands, the old copy is intact and a reload finds it.
        // Nothing has to reclaim the old sectors either, because
        // `find_free_sectors` derives occupancy from the header entries, so they
        // become free as soon as the entry stops pointing at them.
        let sectors_needed = compressed.len().div_ceil(SECTOR_SIZE) as u32;
        let sector_offset = handle
            .header
            .find_free_sectors(sectors_needed, handle.file_sectors);

        // Write chunk data
        let write_result = Self::write_chunk_data(
            &mut handle.file,
            sector_offset,
            &compressed,
            &mut handle.file_sectors,
        )
        .await;
        let loaded_chunk_count = handle.loaded_chunk_count;
        if let Err(error) = write_result {
            // Give back a handle opened only for this save. Returning through
            // `?` used to leave it in the map, and the cleanup that removes it
            // sits after the write -- so a full disk during `save_all_chunks`,
            // which touches every region in the world, leaked one open file per
            // region until the process ran out of descriptors and could no
            // longer open anything at all, sockets included.
            if we_opened_region && loaded_chunk_count == 0 {
                regions.remove(&region_pos);
            }
            return Err(error);
        }

        // Mark the commit pending before any header I/O can yield. If this task
        // is cancelled or the write fails, the cached handle stays dirty so a
        // later flush or shutdown retries it.
        Self::set_pending_header_entry(
            handle,
            index,
            super::format::ChunkEntry::new(sector_offset, compressed.len() as u32, status),
        );

        // If we opened this region and no chunks are loaded from it,
        // write the header and close it immediately
        if we_opened_region && handle.loaded_chunk_count == 0 {
            self.write_header(&mut handle.file, &handle.header).await?;
            handle.header_dirty = false;
            regions.remove(&region_pos);
        }

        Ok(true)
    }

    fn encode_chunk(prepared: PreparedChunkSave) -> io::Result<Vec<u8>> {
        let data = wincode::serialize(&prepared.persistent)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let compressed = zstd::encode_all(&data[..], 3)?;

        if compressed.len() > MAX_CHUNK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Chunk too large: {} bytes (max {})",
                    compressed.len(),
                    MAX_CHUNK_SIZE
                ),
            ));
        }

        Ok(compressed)
    }

    /// Loads a chunk from the appropriate region.
    ///
    /// Automatically opens the region if not already open. The region's reference
    /// count is incremented, so you must call `release_chunk` when done with the chunk.
    ///
    /// Returns `Ok(None)` if the chunk doesn't exist on disk.
    ///
    /// # Arguments
    /// * `pos` - The chunk position
    /// * `min_y` - The minimum Y coordinate of the world
    /// * `height` - The total height of the world
    /// * `level` - Weak reference to the world for Full chunk runtime access
    ///
    /// The region must already be acquired via `acquire_chunk` before calling this.
    pub async fn load_chunk(
        &self,
        pos: ChunkPos,
        min_y: i32,
        height: i32,
        level: Weak<World>,
        thread_pool: &rayon::ThreadPool,
    ) -> io::Result<Option<LoadedChunk>> {
        if height <= 0 || height % 16 != 0 || min_y % 16 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "chunk world range must be section-aligned, got min_y={min_y}, height={height}"
                ),
            ));
        }
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);

        let (compressed, entry) = {
            let mut regions = self.regions.write().await;

            // Get the region (should already be open via acquire_chunk)
            let Some(handle) = regions.get_mut(&region_pos) else {
                log::warn!("load_chunk called without acquire_chunk for region {region_pos:?}");
                return Ok(None);
            };

            // Check if chunk exists
            let entry = handle.header.entries[index];
            if !entry.exists() {
                return Ok(None);
            }

            // Invalid offsets and sizes indicate damage to the region's location
            // table, not a self-contained chunk payload. Do not discard the slot.
            Self::validate_chunk_entry(entry, handle.file_sectors)?;

            // Read chunk data from disk
            let compressed =
                Self::read_chunk_data(&mut handle.file, entry.sector_offset, entry.size_bytes)
                    .await?;
            (compressed, entry)
        };

        // Keep CPU-heavy decoding off the async runtime. Awaiting the Rayon
        // handoff also lets the region-lock waiter woken above make progress.
        let (sender, receiver) = oneshot::channel();
        thread_pool.spawn(move || {
            let result = Self::decode_chunk(compressed, pos, entry.status, min_y, height, level);
            if sender.send(result).is_err() {
                tracing::trace!(
                    chunk = ?pos,
                    "Discarding decoded chunk after its load task was canceled"
                );
            }
        });

        let decoded = receiver
            .await
            .map_err(|_| io::Error::other("chunk decode task ended without returning a result"))?;
        match decoded {
            Ok(loaded) => Ok(Some(loaded)),
            Err(error) => {
                if !self
                    .clear_corrupt_chunk_if_unchanged(region_pos, index, entry)
                    .await?
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "corrupt chunk payload was superseded before it could be removed: {error}"
                        ),
                    ));
                }
                tracing::error!(
                    chunk = ?pos,
                    "Discarded corrupt chunk payload and will regenerate it: {error}",
                );
                Ok(None)
            }
        }
    }

    fn decode_chunk(
        compressed: Vec<u8>,
        pos: ChunkPos,
        status: ChunkStatus,
        min_y: i32,
        height: i32,
        level: Weak<World>,
    ) -> Result<LoadedChunk, CorruptChunkData> {
        // Bounded, not `decode_all`. A zstd frame declares nothing useful about
        // how far it expands, so an unbounded decode lets a crafted or corrupt
        // region file -- a downloaded map, a restored backup, a truncated write
        // -- turn a few megabytes on disk into hundreds of gigabytes in memory
        // and take the process out. The packet decoder already caps its zlib
        // output for exactly this reason; the disk path did not.
        let data = decompress(&compressed[..], MAX_DECOMPRESSED_CHUNK_BYTES)
            .map_err(|error| CorruptChunkData(format!("zstd decode failed: {error}")))?;
        let persistent: PersistentChunk<'_> = wincode::deserialize(&data)
            .map_err(|error| CorruptChunkData(format!("chunk decode failed: {error}")))?;

        ChunkStorage::try_persistent_to_chunk(&persistent, pos, status, min_y, height, level)
            .map_err(|error| CorruptChunkData(format!("chunk materialization failed: {error}")))
    }

    /// Acquires a chunk, incrementing the region's reference count.
    ///
    /// This opens or creates the region file. Call this before loading or
    /// generating a chunk, and call `release_chunk` when done with the chunk.
    ///
    /// Returns `Ok(true)` if the chunk exists on disk, `Ok(false)` if it doesn't.
    #[expect(
        clippy::missing_panics_doc,
        reason = "panic on `just inserted` is unreachable"
    )]
    pub async fn acquire_chunk(&self, pos: ChunkPos) -> io::Result<bool> {
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);

        let mut regions = self.regions.write().await;

        // Get or open/create the region
        let handle = if let Some(handle) = regions.get_mut(&region_pos) {
            handle
        } else {
            // open_region creates the file if it doesn't exist
            let handle = self.open_region(region_pos).await?;
            regions.insert(region_pos, handle);
            #[cfg_attr(
                not(test),
                expect(
                    clippy::expect_used,
                    reason = "inserted on the line above; the entry API cannot be used here because opening a region awaits"
                )
            )]
            let handle = regions.get_mut(&region_pos).expect("just inserted");
            handle
        };

        // Check if chunk exists
        let exists = handle.header.entries[index].exists();

        // Increment ref count
        handle.loaded_chunk_count += 1;

        Ok(exists)
    }

    /// Releases a loaded chunk, decrementing the region's reference count.
    ///
    /// When all chunks from a region are released, the header is saved (if dirty)
    /// and the file handle is closed.
    ///
    /// This must be called for each chunk returned by `load_chunk`.
    pub async fn release_chunk(&self, pos: ChunkPos) -> io::Result<()> {
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);

        let mut regions = self.regions.write().await;

        let should_close = if let Some(handle) = regions.get_mut(&region_pos) {
            handle.loaded_chunk_count = handle.loaded_chunk_count.saturating_sub(1);
            handle.loaded_chunk_count == 0
        } else {
            return Ok(());
        };

        if should_close
            && let Some(mut handle) = regions.remove(&region_pos)
            && handle.header_dirty
        {
            // The header is the commit point for every chunk written into this
            // region since it was opened. Evicting the handle before the write
            // means a failure here -- a full disk, an I/O error -- drops the only
            // reference to that pending commit, and `flush_all` can no longer
            // find it to retry. The sectors are on disk but nothing points at
            // them, so the next load reports the chunks as absent and the terrain
            // silently regenerates over whatever the player built. Put the handle
            // back instead.
            if let Err(error) = self.write_header(&mut handle.file, &handle.header).await {
                regions.insert(region_pos, handle);
                return Err(error);
            }
            handle.header_dirty = false;
        }

        Ok(())
    }

    /// Checks if a chunk exists on disk without loading it.
    pub async fn chunk_exists(&self, pos: ChunkPos) -> io::Result<bool> {
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);

        let regions = self.regions.write().await;

        // Check cached header first
        if let Some(handle) = regions.get(&region_pos) {
            return Ok(handle.header.entries[index].exists());
        }

        drop(regions);

        // Need to read header from disk
        let path = self.region_path(region_pos);
        if !path.exists() {
            return Ok(false);
        }

        let mut file = File::open(&path).await?;

        // Skip magic + version
        file.seek(io::SeekFrom::Start(FILE_HEADER_SIZE as u64))
            .await?;

        // Read just the one entry we need (8 bytes at index * 8)
        file.seek(io::SeekFrom::Current((index * 8) as i64)).await?;
        let mut entry_bytes = [0u8; 8];
        file.read_exact(&mut entry_bytes).await?;

        let Some(entry) = super::format::ChunkEntry::from_bytes(entry_bytes) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "chunk table entry has an invalid status byte",
            ));
        };
        Ok(entry.exists())
    }

    /// Flushes all dirty headers to disk.
    pub async fn flush_all(&self) -> io::Result<()> {
        let mut regions = self.regions.write().await;

        for handle in regions.values_mut() {
            if handle.header_dirty {
                self.write_header(&mut handle.file, &handle.header).await?;
                handle.header_dirty = false;
            }
        }

        Ok(())
    }

    /// Flushes all dirty headers and closes all region file handles.
    ///
    /// This should be called during graceful shutdown after all chunks have been saved.
    /// It ensures all data is persisted and file handles are properly closed.
    pub async fn close_all(&self) -> io::Result<()> {
        let mut regions = self.regions.write().await;

        // Write every region even after an earlier failure, while retaining
        // failed dirty handles so a later shutdown attempt can retry them.
        let mut first_error = None;
        for (region_pos, mut handle) in mem::take(&mut *regions) {
            if handle.header_dirty {
                match self.write_header(&mut handle.file, &handle.header).await {
                    Ok(()) => {}
                    Err(error) => {
                        first_error.get_or_insert(error);
                        regions.insert(region_pos, handle);
                    }
                }
            }
            // Successful and already-clean handles are dropped here.
        }

        first_error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        io::Write as _,
        path::Path,
        process, slice,
        sync::{
            Arc, Weak,
            atomic::{AtomicU64, Ordering},
        },
    };

    use super::super::format::CHUNKS_PER_REGION;
    use super::*;
    use crate::chunk_saver::{PersistentChunk, PersistentLightData};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn test_directory(name: &str) -> PathBuf {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "foton-region-manager-{name}-{}-{sequence}",
            process::id()
        ))
    }

    async fn write_test_region(
        directory: &Path,
        pos: ChunkPos,
        payload: &[u8],
        declared_size: u32,
    ) -> io::Result<()> {
        fs::create_dir_all(directory).await?;
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let path = directory.join(region_pos.filename());
        let mut file = File::create(path).await?;
        let mut file_header = [0u8; FILE_HEADER_SIZE];
        file_header[0..4].copy_from_slice(&REGION_MAGIC);
        file_header[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        file.write_all(&file_header).await?;

        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);
        let mut header = RegionHeader::new();
        header.entries[index] =
            ChunkEntry::new(FIRST_DATA_SECTOR, declared_size, ChunkStatus::Empty);
        file.write_all(&header.to_bytes()).await?;
        file.seek(io::SeekFrom::Start(
            u64::from(FIRST_DATA_SECTOR) * SECTOR_SIZE as u64,
        ))
        .await?;
        file.write_all(payload).await?;
        file.flush().await
    }

    #[test]
    fn sparse_region_size_does_not_drive_validation_memory() {
        RegionManager::validate_region_entries(&RegionHeader::new(), u32::MAX)
            .expect("an empty sparse region header has no invalid allocations");
    }

    #[test]
    fn interval_validation_still_rejects_overlapping_chunks() {
        let mut header = RegionHeader::new();
        header.entries[0] = ChunkEntry::new(FIRST_DATA_SECTOR, 1, ChunkStatus::Empty);
        header.entries[1] = ChunkEntry::new(FIRST_DATA_SECTOR, 1, ChunkStatus::Empty);

        let result = RegionManager::validate_region_entries(&header, FIRST_DATA_SECTOR + 1);
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::InvalidData));
    }

    fn test_thread_pool() -> rayon::ThreadPool {
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("test thread pool should build")
    }

    async fn assert_slot_exists_on_disk(directory: &Path, pos: ChunkPos) {
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let path = directory.join(region_pos.filename());
        let mut file = File::open(path)
            .await
            .expect("test region should remain readable");
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);
        file.seek(io::SeekFrom::Start(
            FILE_HEADER_SIZE as u64 + (index * 8) as u64,
        ))
        .await
        .expect("chunk table entry should be seekable");
        let mut bytes = [0; 8];
        file.read_exact(&mut bytes)
            .await
            .expect("chunk table entry should be readable");
        assert!(ChunkEntry::from_bytes(bytes).is_some_and(|entry| entry.exists()));
    }

    async fn assert_slot_is_empty_on_disk(directory: &Path, pos: ChunkPos) {
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let path = directory.join(region_pos.filename());
        let mut file = File::open(path)
            .await
            .expect("test region should remain readable");
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);
        file.seek(io::SeekFrom::Start(
            FILE_HEADER_SIZE as u64 + (index * 8) as u64,
        ))
        .await
        .expect("chunk table entry should be seekable");
        let mut bytes = [0; 8];
        file.read_exact(&mut bytes)
            .await
            .expect("chunk table entry should be readable");
        assert!(ChunkEntry::from_bytes(bytes).is_some_and(|entry| !entry.exists()));
    }

    #[tokio::test]
    async fn a_pending_header_commit_survives_cancellation_before_write() {
        let directory = test_directory("pending-header");
        let pos = ChunkPos::new(0, 0);
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let manager = RegionManager::new(&directory);
        let mut handle = manager
            .create_region(region_pos)
            .await
            .expect("test region should be created");
        handle
            .file
            .set_len(u64::from(FIRST_DATA_SECTOR + 1) * SECTOR_SIZE as u64)
            .await
            .expect("test region should contain the pending sector");
        handle.file_sectors = FIRST_DATA_SECTOR + 1;
        manager.regions.write().await.insert(region_pos, handle);

        let entry = ChunkEntry::new(FIRST_DATA_SECTOR, 1, ChunkStatus::Empty);
        {
            let mut regions = manager.regions.write().await;
            let handle = regions
                .get_mut(&region_pos)
                .expect("test region should remain cached");
            RegionManager::set_pending_header_entry(handle, 0, entry);
        }

        manager
            .flush_all()
            .await
            .expect("a later flush should commit the pending header");
        drop(manager);

        let reopened = RegionManager::new(&directory);
        assert!(
            reopened
                .acquire_chunk(pos)
                .await
                .expect("the committed header should reopen")
        );
        reopened
            .release_chunk(pos)
            .await
            .expect("reopened region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn close_all_keeps_a_dirty_handle_when_its_header_write_fails() {
        let directory = test_directory("close-header-retry");
        let pos = ChunkPos::new(0, 0);
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let manager = RegionManager::new(&directory);
        let mut handle = manager
            .create_region(region_pos)
            .await
            .expect("test region should be created");
        handle
            .file
            .set_len(u64::from(FIRST_DATA_SECTOR + 1) * SECTOR_SIZE as u64)
            .await
            .expect("test region should contain the pending sector");
        handle.file_sectors = FIRST_DATA_SECTOR + 1;
        RegionManager::set_pending_header_entry(
            &mut handle,
            0,
            ChunkEntry::new(FIRST_DATA_SECTOR, 1, ChunkStatus::Empty),
        );
        manager.regions.write().await.insert(region_pos, handle);
        manager.header_write_failures.store(1, Ordering::Release);

        let error = manager
            .close_all()
            .await
            .expect_err("the injected header failure should be reported");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        {
            let regions = manager.regions.read().await;
            let retained = regions
                .get(&region_pos)
                .expect("the failed dirty handle must remain retryable");
            assert!(retained.header_dirty);
        }

        manager
            .close_all()
            .await
            .expect("a later close should retry and commit the header");
        assert!(manager.regions.read().await.is_empty());
        let reopened = RegionManager::new(&directory);
        assert!(
            reopened
                .chunk_exists(pos)
                .await
                .expect("the retried header should be readable")
        );

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn failed_corrupt_slot_header_write_remains_dirty_and_retryable() {
        let directory = test_directory("corrupt-header-retry");
        let pos = ChunkPos::new(0, 0);
        let payload = b"this is not a zstd frame";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");
        let manager = RegionManager::new(&directory);
        manager.header_write_failures.store(1, Ordering::Release);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );

        let Err(error) = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
        else {
            panic!("the injected header failure should be reported");
        };
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            !manager
                .chunk_exists(pos)
                .await
                .expect("the pending clear should remain visible in memory")
        );
        manager
            .flush_all()
            .await
            .expect("a later flush should retry the pending clear");
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_is_empty_on_disk(&directory, pos).await;

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn cancelled_corrupt_slot_header_write_remains_dirty_and_retryable() {
        let directory = test_directory("corrupt-header-cancellation");
        let pos = ChunkPos::new(0, 0);
        let payload = b"this is not a zstd frame";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");
        let manager = Arc::new(RegionManager::new(&directory));
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        manager.pause_header_writes.store(true, Ordering::Release);
        let write_started = manager.header_write_started.notified();
        let task_manager = Arc::clone(&manager);
        let thread_pool = Arc::new(test_thread_pool());
        let task_pool = Arc::clone(&thread_pool);
        let load = tokio::spawn(async move {
            task_manager
                .load_chunk(pos, 0, 16, Weak::new(), &task_pool)
                .await
        });
        write_started.await;

        load.abort();
        let Err(error) = load.await else {
            panic!("the paused load should be cancelled");
        };
        assert!(error.is_cancelled());
        manager.pause_header_writes.store(false, Ordering::Release);
        assert!(
            !manager
                .chunk_exists(pos)
                .await
                .expect("the cancelled clear should remain visible in memory")
        );
        manager
            .flush_all()
            .await
            .expect("a later flush should commit the cancelled clear");
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_is_empty_on_disk(&directory, pos).await;

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[test]
    fn maximum_sparse_region_entries_are_checked_by_interval() {
        let mut header = RegionHeader::new();
        header.entries[0] = ChunkEntry::new(u32::MAX - 2, 1, ChunkStatus::Empty);
        header.entries[1] = ChunkEntry::new(u32::MAX - 2, 1, ChunkStatus::Empty);

        let error = RegionManager::validate_region_entries(&header, u32::MAX)
            .expect_err("overlapping intervals must be rejected");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("overlaps"));
    }

    #[tokio::test]
    async fn oversized_sparse_region_is_rejected_before_sector_tracking() {
        let directory = test_directory("oversized-sparse");
        let pos = ChunkPos::new(0, 0);
        let manager = RegionManager::new(&directory);
        manager
            .create_region(RegionPos::from_chunk(pos.0.x, pos.0.y))
            .await
            .expect("test region should be created");

        let path = directory.join(RegionPos::from_chunk(0, 0).filename());
        let file = OpenOptions::new()
            .write(true)
            .open(&path)
            .await
            .expect("test region should reopen");
        file.set_len(MAX_REGION_FILE_SIZE + 1)
            .await
            .expect("sparse file should extend without allocating its contents");

        let Err(error) = manager.acquire_chunk(pos).await else {
            panic!("a region larger than the format capacity must be rejected");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn chunk_write_rejects_sector_overflow_before_extending_a_sparse_file() {
        let directory = test_directory("write-sector-overflow");
        fs::create_dir_all(&directory)
            .await
            .expect("test directory should be created");
        let path = directory.join("overflow.srg");
        let mut file = File::create(&path)
            .await
            .expect("test file should be created");
        let mut file_sectors = FIRST_DATA_SECTOR;

        let error = RegionManager::write_chunk_data(&mut file, u32::MAX, &[1], &mut file_sectors)
            .await
            .expect_err("a sector range outside the format must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(file_sectors, FIRST_DATA_SECTOR);
        assert_eq!(
            file.metadata()
                .await
                .expect("test file metadata should be readable")
                .len(),
            0,
            "validation must happen before seek or write"
        );
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn new_region_reports_a_failed_directory_commit_and_can_be_reopened() {
        let directory = test_directory("new-region-directory-sync");
        fs::create_dir_all(&directory)
            .await
            .expect("test directory should be created");
        let pos = ChunkPos::new(0, 0);
        let manager = RegionManager::new(&directory);
        manager
            .directory_sync_failure
            .lock()
            .replace(directory.clone());

        let error = manager
            .acquire_chunk(pos)
            .await
            .expect_err("a failed namespace commit must fail region creation");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            fs::try_exists(manager.region_path(RegionPos::from_chunk(0, 0)))
                .await
                .expect("the fully written file path should be checkable")
        );

        manager.directory_sync_failure.lock().take();
        assert!(
            !manager
                .acquire_chunk(pos)
                .await
                .expect("the durable file contents should reopen on retry")
        );
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn creating_a_missing_region_directory_commits_each_namespace_level() {
        let root = test_directory("new-region-directory-create");
        fs::create_dir_all(&root)
            .await
            .expect("test root should be created");
        let directory = root.join("regions");
        let pos = ChunkPos::new(0, 0);
        let manager = RegionManager::new(&directory);

        assert!(
            !manager
                .acquire_chunk(pos)
                .await
                .expect("new region should be created")
        );
        assert_eq!(
            *manager.directory_syncs.lock(),
            [root.clone(), directory.clone()]
        );
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(root)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn valid_append_history_beyond_live_table_size_still_opens() {
        let directory = test_directory("append-history");
        let pos = ChunkPos::new(0, 0);
        let manager = RegionManager::new(&directory);
        manager
            .create_region(RegionPos::from_chunk(pos.0.x, pos.0.y))
            .await
            .expect("test region should be created");

        let live_table_bound = ((FILE_HEADER_SIZE + CHUNK_TABLE_SIZE).div_ceil(SECTOR_SIZE)
            + CHUNKS_PER_REGION * MAX_CHUNK_SIZE.div_ceil(SECTOR_SIZE))
            as u64
            * SECTOR_SIZE as u64;
        let path = directory.join(RegionPos::from_chunk(0, 0).filename());
        let file = OpenOptions::new()
            .write(true)
            .open(&path)
            .await
            .expect("test region should reopen");
        file.set_len(live_table_bound + 1)
            .await
            .expect("append history should extend without allocating its contents");

        assert!(
            !manager
                .acquire_chunk(pos)
                .await
                .expect("valid append history should still open")
        );
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn version_backup_collision_preserves_both_region_files() {
        let directory = test_directory("version-backup-collision");
        fs::create_dir_all(&directory)
            .await
            .expect("test directory should be created");
        let pos = ChunkPos::new(0, 0);
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let region_path = directory.join(region_pos.filename());
        let old_version = FORMAT_VERSION - 1;
        let mut old_region = vec![0u8; FILE_HEADER_SIZE];
        old_region[0..4].copy_from_slice(&REGION_MAGIC);
        old_region[4..6].copy_from_slice(&old_version.to_le_bytes());
        fs::write(&region_path, &old_region)
            .await
            .expect("old region should be written");
        let first_backup = region_path.with_extension(format!("srg.v{old_version}.bak"));
        fs::write(&first_backup, b"first backup")
            .await
            .expect("existing backup should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            !manager
                .acquire_chunk(pos)
                .await
                .expect("old region should be backed up and recreated")
        );
        assert_eq!(
            fs::read(&first_backup)
                .await
                .expect("first backup should remain readable"),
            b"first backup"
        );
        let second_backup = region_path.with_extension(format!("srg.v{old_version}.bak.1"));
        assert_eq!(
            fs::read(second_backup)
                .await
                .expect("old region should use the next free backup name"),
            old_region
        );
        assert_eq!(
            *manager.directory_syncs.lock(),
            [directory.clone(), directory.clone(), directory.clone()],
            "the hard link, original removal, and replacement creation each mutate the directory"
        );

        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn incompatible_backup_is_synced_before_the_original_is_removed() {
        let directory = test_directory("backup-link-directory-sync");
        fs::create_dir_all(&directory)
            .await
            .expect("test directory should be created");
        let pos = ChunkPos::new(0, 0);
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let region_path = directory.join(region_pos.filename());
        let old_version = FORMAT_VERSION - 1;
        let mut old_region = vec![0u8; FILE_HEADER_SIZE];
        old_region[0..4].copy_from_slice(&REGION_MAGIC);
        old_region[4..6].copy_from_slice(&old_version.to_le_bytes());
        fs::write(&region_path, &old_region)
            .await
            .expect("old region should be written");
        let manager = RegionManager::new(&directory);
        manager
            .directory_sync_failure
            .lock()
            .replace(directory.clone());

        let error = manager
            .acquire_chunk(pos)
            .await
            .expect_err("a failed backup-link commit should stop replacement");
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(
            fs::try_exists(&region_path)
                .await
                .expect("source path should be checked")
        );
        let backup = region_path.with_extension(format!("srg.v{old_version}.bak"));
        assert!(
            fs::try_exists(backup)
                .await
                .expect("backup path should be checked")
        );

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn oversized_incompatible_version_is_backed_up_before_size_rejection() {
        let directory = test_directory("oversized-version-backup");
        fs::create_dir_all(&directory)
            .await
            .expect("test directory should be created");
        let pos = ChunkPos::new(0, 0);
        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let region_path = directory.join(region_pos.filename());
        let old_version = FORMAT_VERSION - 1;
        let mut old_header = [0u8; FILE_HEADER_SIZE];
        old_header[0..4].copy_from_slice(&REGION_MAGIC);
        old_header[4..6].copy_from_slice(&old_version.to_le_bytes());
        fs::write(&region_path, old_header)
            .await
            .expect("old region should be written");
        let first_backup = region_path.with_extension(format!("srg.v{old_version}.bak"));
        fs::write(&first_backup, b"first backup")
            .await
            .expect("existing backup should be written");
        let file = OpenOptions::new()
            .write(true)
            .open(&region_path)
            .await
            .expect("old region should reopen");
        file.set_len(MAX_REGION_FILE_SIZE + 1)
            .await
            .expect("sparse old region should extend without allocating its contents");

        let manager = RegionManager::new(&directory);
        assert!(
            !manager
                .acquire_chunk(pos)
                .await
                .expect("oversized old region should be backed up and recreated")
        );
        assert_eq!(
            fs::read(&first_backup)
                .await
                .expect("first backup should remain readable"),
            b"first backup"
        );
        let second_backup = region_path.with_extension(format!("srg.v{old_version}.bak.1"));
        assert_eq!(
            fs::metadata(&second_backup)
                .await
                .expect("oversized old region should be preserved")
                .len(),
            MAX_REGION_FILE_SIZE + 1
        );
        let mut backup = File::open(&second_backup)
            .await
            .expect("oversized backup should remain readable");
        let mut backed_up_header = [0u8; FILE_HEADER_SIZE];
        backup
            .read_exact(&mut backed_up_header)
            .await
            .expect("backup header should remain readable");
        assert_eq!(&backed_up_header[0..4], &REGION_MAGIC);
        assert_eq!(
            u16::from_le_bytes([backed_up_header[4], backed_up_header[5]]),
            old_version
        );

        manager
            .release_chunk(pos)
            .await
            .expect("new region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    /// A decompression bomb is refused instead of being expanded into memory.
    ///
    /// A zstd frame says nothing useful about how far it expands, so an
    /// unbounded decode turns a few megabytes of region file -- a downloaded
    /// map, a restored backup -- into as much memory as the attacker likes.
    #[test]
    fn a_decompression_bomb_is_refused_rather_than_expanded() {
        let mut bomb = Vec::new();
        {
            let block = vec![0u8; 1024 * 1024];
            let mut encoder = zstd::stream::Encoder::new(&mut bomb, 3)
                .expect("the bomb encoder should initialize");
            for _ in 0..=(MAX_DECOMPRESSED_CHUNK_BYTES / block.len()) {
                encoder
                    .write_all(&block)
                    .expect("the bomb should compress progressively");
            }
            encoder.finish().expect("the bomb should finish");
        }
        assert!(
            bomb.len() < 1024 * 1024,
            "the point of the test is that a small payload expands hugely, but              this one is {} bytes",
            bomb.len()
        );

        assert!(
            decompress(&bomb, MAX_DECOMPRESSED_CHUNK_BYTES).is_err(),
            "a payload over the per-chunk ceiling must be refused"
        );

        // A payload of a realistic size still round-trips.
        let ordinary = vec![7u8; 256 * 1024];
        let packed = zstd::encode_all(ordinary.as_slice(), 3).expect("compresses");
        assert_eq!(
            decompress(&packed, MAX_DECOMPRESSED_CHUNK_BYTES)
                .expect("a normal chunk is nowhere near the ceiling"),
            ordinary
        );
    }

    #[tokio::test]
    async fn invalid_zstd_payload_is_removed_for_regeneration() {
        let directory = test_directory("zstd");
        let pos = ChunkPos::new(0, 0);
        let payload = b"this is not a zstd frame";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let loaded = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
            .expect("corrupt payload should be handled");
        assert!(loaded.is_none());
        assert!(
            !manager
                .chunk_exists(pos)
                .await
                .expect("header should be readable")
        );
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");

        let reopened = RegionManager::new(&directory);
        assert!(
            !reopened
                .chunk_exists(pos)
                .await
                .expect("header should be flushed")
        );
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn semantically_invalid_complete_payload_is_removed_for_regeneration() {
        let directory = test_directory("semantic");
        let pos = ChunkPos::new(0, 0);
        let persistent = PersistentChunk {
            last_modified: 0,
            block_states: Vec::new(),
            biomes: Vec::new(),
            sections: Vec::new(),
            block_entities: Vec::new(),
            entities: Vec::new(),
            block_ticks: Vec::new(),
            fluid_ticks: Vec::new(),
            heightmaps: Vec::new(),
            light: PersistentLightData::default(),
            carving_mask: None,
            postprocessing: Vec::new(),
            structure_starts: Vec::new(),
            structure_references: Vec::new(),
            pois: Vec::new(),
        };
        let encoded = wincode::serialize(&persistent).expect("test chunk should encode");
        let payload = zstd::encode_all(encoded.as_slice(), 1).expect("test chunk should compress");
        write_test_region(&directory, pos, &payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let loaded = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
            .expect("semantic corruption should be handled");
        assert!(loaded.is_none());
        assert!(
            !manager
                .chunk_exists(pos)
                .await
                .expect("slot should be cleared")
        );
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn incomplete_payload_read_is_an_error_and_keeps_slot() {
        let directory = test_directory("short-read");
        let pos = ChunkPos::new(0, 0);
        write_test_region(&directory, pos, &[1, 2, 3], 128)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let Err(error) = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
        else {
            panic!("short filesystem read must not be treated as payload corruption");
        };
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert!(manager.chunk_exists(pos).await.expect("slot should remain"));
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_exists_on_disk(&directory, pos).await;
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn invalid_world_geometry_is_not_classified_as_chunk_corruption() {
        let directory = test_directory("invalid-world-range");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let Err(error) = manager
            .load_chunk(pos, 1, 16, Weak::new(), &test_thread_pool())
            .await
        else {
            panic!("invalid world geometry must fail before decoding the chunk");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(manager.chunk_exists(pos).await.expect("slot should remain"));
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_exists_on_disk(&directory, pos).await;
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn invalid_chunk_status_byte_is_a_structural_error_and_is_preserved() {
        let directory = test_directory("invalid-status");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let path = directory.join(region_pos.filename());
        let mut file = OpenOptions::new()
            .write(true)
            .open(&path)
            .await
            .expect("test region should reopen for corruption");
        file.seek(io::SeekFrom::Start(FILE_HEADER_SIZE as u64 + 7))
            .await
            .expect("status byte should be seekable");
        file.write_all(&[u8::MAX])
            .await
            .expect("status byte should be writable");
        file.flush().await.expect("status byte should be flushed");
        drop(file);

        let manager = RegionManager::new(&directory);
        let Err(error) = manager.acquire_chunk(pos).await else {
            panic!("invalid status byte must reject the region header");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let mut file = File::open(path)
            .await
            .expect("rejected region should remain readable");
        file.seek(io::SeekFrom::Start(FILE_HEADER_SIZE as u64 + 7))
            .await
            .expect("status byte should remain seekable");
        let mut status = [0];
        file.read_exact(&mut status)
            .await
            .expect("status byte should remain readable");
        assert_eq!(status[0], u8::MAX);

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }
    #[tokio::test]
    async fn a_chunk_that_fails_to_decode_is_copied_aside_before_its_slot_is_cleared() {
        // Clearing the slot is what keeps the server usable, but the bytes it
        // replaces are the only copy of whatever a player built in that column.
        // The nesting ceiling made this reachable on purpose rather than only by
        // disk corruption, so losing them silently is no longer theoretical.
        let directory = test_directory("quarantine-corrupt");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let loaded = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
            .expect("a corrupt chunk reports as absent, not as an error");
        assert!(
            loaded.is_none(),
            "the corrupt slot must not produce a chunk"
        );
        assert!(
            !manager.chunk_exists(pos).await.expect("slot should answer"),
            "the corrupt slot must be cleared so worldgen can refill the column"
        );

        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(
            RegionPos::local_chunk_pos(pos.0.x, pos.0.y).0,
            RegionPos::local_chunk_pos(pos.0.x, pos.0.y).1,
        );
        let quarantined = directory
            .join("corrupt")
            .join(format!("{}.{index}.chunk", region_pos.filename()));
        let kept = fs::read(&quarantined)
            .await
            .expect("the bytes must be copied aside before the slot is cleared");
        assert_eq!(
            kept, payload,
            "the copy must be the bytes that were on disk, not a rewrite of them"
        );

        // A cleared slot is refilled by worldgen, so a second corruption at the
        // same index carries regenerated terrain. Overwriting would trade the
        // player's build for a hillside.
        write_test_region(&directory, pos, b"second payload", 14)
            .await
            .expect("test region should be rewritten");
        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should reopen")
        );
        assert!(
            manager
                .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
                .await
                .expect("the second corrupt chunk also reports as absent")
                .is_none()
        );
        assert_eq!(
            fs::read(&quarantined)
                .await
                .expect("the first copy must still be there"),
            payload,
            "the first copy is the valuable one and must not be overwritten"
        );
        assert!(
            fs::try_exists(
                directory
                    .join("corrupt")
                    .join(format!("{}.{index}.1.chunk", region_pos.filename()))
            )
            .await
            .unwrap_or(false),
            "the second copy must be set aside next to the first"
        );

        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn failed_quarantine_write_keeps_the_original_slot() {
        let directory = test_directory("quarantine-write-failure");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");
        fs::write(directory.join("corrupt"), b"not a directory")
            .await
            .expect("quarantine blocker should be written");

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let Err(error) = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
        else {
            panic!("failed quarantine must abort corrupt-slot removal");
        };
        assert_ne!(error.kind(), io::ErrorKind::InvalidData);
        assert!(manager.chunk_exists(pos).await.expect("slot should remain"));
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_exists_on_disk(&directory, pos).await;

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn failed_new_quarantine_parent_sync_keeps_the_original_slot() {
        let directory = test_directory("quarantine-parent-sync-failure");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let manager = RegionManager::new(&directory);
        manager
            .directory_sync_failure
            .lock()
            .replace(directory.clone());
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        let Err(error) = manager
            .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
            .await
        else {
            panic!("a failed quarantine-directory commit must abort slot removal");
        };
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(manager.chunk_exists(pos).await.expect("slot should remain"));
        assert_eq!(
            *manager.directory_syncs.lock(),
            slice::from_ref(&directory),
            "the new quarantine directory must be committed before a file is published inside it"
        );

        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_exists_on_disk(&directory, pos).await;
        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }

    #[tokio::test]
    async fn exhausted_quarantine_names_keep_existing_backups_and_the_slot() {
        let directory = test_directory("quarantine-collisions");
        let pos = ChunkPos::new(0, 0);
        let payload = b"payload must not be decoded";
        write_test_region(&directory, pos, payload, payload.len() as u32)
            .await
            .expect("test region should be written");

        let region_pos = RegionPos::from_chunk(pos.0.x, pos.0.y);
        let (local_x, local_z) = RegionPos::local_chunk_pos(pos.0.x, pos.0.y);
        let index = RegionHeader::chunk_index(local_x, local_z);
        let corrupt = directory.join("corrupt");
        fs::create_dir_all(&corrupt)
            .await
            .expect("quarantine directory should be created");
        for attempt in 0..MAX_QUARANTINED_COPIES_PER_SLOT {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!(".{attempt}")
            };
            fs::write(
                corrupt.join(format!("{}.{index}{suffix}.chunk", region_pos.filename())),
                format!("existing-{attempt}"),
            )
            .await
            .expect("existing quarantine copy should be written");
        }

        let manager = RegionManager::new(&directory);
        assert!(
            manager
                .acquire_chunk(pos)
                .await
                .expect("region should open")
        );
        assert!(
            manager
                .load_chunk(pos, 0, 16, Weak::new(), &test_thread_pool())
                .await
                .is_err(),
            "an exhausted collision-safe namespace must not clear the slot"
        );
        assert!(manager.chunk_exists(pos).await.expect("slot should remain"));
        for attempt in 0..MAX_QUARANTINED_COPIES_PER_SLOT {
            let suffix = if attempt == 0 {
                String::new()
            } else {
                format!(".{attempt}")
            };
            let path = corrupt.join(format!("{}.{index}{suffix}.chunk", region_pos.filename()));
            assert_eq!(
                fs::read_to_string(path)
                    .await
                    .expect("existing quarantine copy should remain readable"),
                format!("existing-{attempt}")
            );
        }
        manager
            .release_chunk(pos)
            .await
            .expect("region should release");
        assert_slot_exists_on_disk(&directory, pos).await;

        fs::remove_dir_all(directory)
            .await
            .expect("test directory should be removable");
    }
}
