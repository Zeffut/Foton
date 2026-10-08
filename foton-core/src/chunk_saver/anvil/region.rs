//! Reader for vanilla Anvil region files (`r.X.Z.mca`).
//!
//! Vanilla parity: `RegionFile` and `RegionFileVersion`. A region is two 4 KiB
//! tables (sector location, then timestamps) followed by 4 KiB sectors. A chunk
//! record is a big-endian `i32` length, a compression id, and the payload; the
//! length counts the id byte. Ids 1-4 are gzip, zlib, none and LZ4. Bit 7 set
//! moves the payload into a sibling `c.X.Z.mcc` file, which vanilla does for
//! chunks too large for the 255-sector limit.

use std::fs::File;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use flate2::read::{GzDecoder, ZlibDecoder};
use lz4_flex::block::decompress as decompress_block;
use thiserror::Error;

/// Anvil's sector size.
const SECTOR_BYTES: u64 = 4096;

/// Both header tables.
const HEADER_BYTES: u64 = 2 * SECTOR_BYTES;

/// Bytes in front of a chunk payload: the length and the compression id.
const RECORD_HEADER_BYTES: u64 = 5;

/// Chunks per region side.
const REGION_WIDTH: usize = 32;

/// Set on the compression id when the payload lives in a `.mcc` file.
const EXTERNAL_FLAG: u8 = 0x80;

/// The most one chunk may expand to.
///
/// The same ceiling Foton's own chunk decoder applies: a legitimate chunk is a
/// few hundred kilobytes, and a crafted one must not turn into an out-of-memory
/// kill on a small host.
pub(super) const MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

/// Largest `.mcc` file read into memory. Compressed, so well below the above.
const MAX_EXTERNAL_BYTES: u64 = 256 * 1024 * 1024;

/// Header of one `LZ4BlockOutputStream` block: magic, token, then three
/// little-endian `i32`s (compressed length, original length, checksum).
const LZ4_BLOCK_HEADER_BYTES: usize = 21;
const LZ4_BLOCK_MAGIC: &[u8; 8] = b"LZ4Block";
const LZ4_METHOD_MASK: u8 = 0xF0;
const LZ4_METHOD_RAW: u8 = 0x10;
const LZ4_METHOD_COMPRESSED: u8 = 0x20;

/// Why a region or one of its chunks could not be read.
#[derive(Debug, Error)]
pub enum RegionError {
    /// The file system refused a read.
    #[error("{0}")]
    Io(#[from] io::Error),
    /// The bytes do not form a valid region record or compressed stream.
    #[error("corrupt region data: {0}")]
    Corrupt(String),
    /// A compression id vanilla does not define.
    #[error("unsupported chunk compression id {0}")]
    UnsupportedCompression(u8),
}

/// How a chunk payload is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// RFC 1952.
    Gzip,
    /// RFC 1950, vanilla's default.
    Zlib,
    /// Stored as is.
    None,
    /// lz4-java's block stream, which is not the LZ4 frame format.
    Lz4,
}

impl Compression {
    /// Maps a record's compression id, with the external flag already cleared.
    #[must_use]
    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Self::Gzip),
            2 => Some(Self::Zlib),
            3 => Some(Self::None),
            4 => Some(Self::Lz4),
            _ => None,
        }
    }
}

/// One chunk record as stored, before decompression.
#[derive(Debug)]
pub struct RawChunk {
    /// How [`Self::data`] is compressed.
    pub compression: Compression,
    /// The stored payload.
    pub data: Vec<u8>,
    /// Last-modified time in epoch seconds, from the timestamp table.
    pub timestamp: u32,
}

impl RawChunk {
    /// Returns the NBT bytes.
    ///
    /// # Errors
    /// Returns [`RegionError::Corrupt`] if the stream is malformed or expands
    /// past [`MAX_DECOMPRESSED_BYTES`].
    pub fn decompress(&self) -> Result<Vec<u8>, RegionError> {
        decompress(self.compression, &self.data, MAX_DECOMPRESSED_BYTES)
    }
}

/// An open region file with its location table loaded.
pub struct AnvilRegion {
    file: File,
    directory: PathBuf,
    region_x: i32,
    region_z: i32,
    locations: Vec<u32>,
    timestamps: Vec<u32>,
    length: u64,
}

impl AnvilRegion {
    /// Opens `path`, which must be named `r.<x>.<z>.mca`.
    ///
    /// A file shorter than the two tables is a region vanilla created and never
    /// filled; it reads as empty.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or is not named as a region.
    pub fn open(path: &Path) -> Result<Self, RegionError> {
        let (region_x, region_z) = Self::coordinates_from_path(path).ok_or_else(|| {
            RegionError::Corrupt(format!("{} is not named r.<x>.<z>.mca", path.display()))
        })?;
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut locations = vec![0; REGION_WIDTH * REGION_WIDTH];
        let mut timestamps = vec![0; REGION_WIDTH * REGION_WIDTH];
        if length >= HEADER_BYTES {
            let mut header = vec![0u8; HEADER_BYTES as usize];
            file.read_exact(&mut header)?;
            let (location_bytes, timestamp_bytes) = header.split_at(SECTOR_BYTES as usize);
            for (slot, bytes) in locations.iter_mut().zip(location_bytes.as_chunks::<4>().0) {
                *slot = u32::from_be_bytes(*bytes);
            }
            for (slot, bytes) in timestamps
                .iter_mut()
                .zip(timestamp_bytes.as_chunks::<4>().0)
            {
                *slot = u32::from_be_bytes(*bytes);
            }
        }
        Ok(Self {
            file,
            directory: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            region_x,
            region_z,
            locations,
            timestamps,
            length,
        })
    }

    /// Parses the region coordinates out of `r.<x>.<z>.mca`.
    #[must_use]
    pub fn coordinates_from_path(path: &Path) -> Option<(i32, i32)> {
        let name = path.file_name()?.to_str()?;
        let coordinates = name.strip_prefix("r.")?.strip_suffix(".mca")?;
        let (x, z) = coordinates.split_once('.')?;
        Some((x.parse().ok()?, z.parse().ok()?))
    }

    /// Region X coordinate.
    #[must_use]
    pub const fn region_x(&self) -> i32 {
        self.region_x
    }

    /// Region Z coordinate.
    #[must_use]
    pub const fn region_z(&self) -> i32 {
        self.region_z
    }

    const fn slot(local_x: usize, local_z: usize) -> usize {
        local_z * REGION_WIDTH + local_x
    }

    /// Whether the location table lists a chunk at these region-local coordinates.
    #[must_use]
    pub fn contains(&self, local_x: usize, local_z: usize) -> bool {
        local_x < REGION_WIDTH
            && local_z < REGION_WIDTH
            && self.locations[Self::slot(local_x, local_z)] != 0
    }

    /// Reads one chunk record without decompressing it.
    ///
    /// Returns `Ok(None)` when the table has no entry for the chunk.
    ///
    /// # Errors
    /// Returns an error if the record or its `.mcc` file is truncated, out of
    /// bounds or uses an unknown compression id.
    pub fn read_raw(
        &mut self,
        local_x: usize,
        local_z: usize,
    ) -> Result<Option<RawChunk>, RegionError> {
        if !self.contains(local_x, local_z) {
            return Ok(None);
        }
        let slot = Self::slot(local_x, local_z);
        let location = self.locations[slot];
        let sector = u64::from(location >> 8);
        let sector_count = u64::from(location & 0xFF);
        if sector < 2 || sector_count == 0 {
            return Err(RegionError::Corrupt(format!(
                "location entry {location:#010x} points into the header or is empty"
            )));
        }

        let start = sector * SECTOR_BYTES;
        if start + RECORD_HEADER_BYTES > self.length {
            return Err(RegionError::Corrupt(
                "chunk record starts past the end of the file".to_owned(),
            ));
        }
        self.file.seek(SeekFrom::Start(start))?;
        let mut record_header = [0u8; RECORD_HEADER_BYTES as usize];
        self.file.read_exact(&mut record_header)?;
        let length = i32::from_be_bytes([
            record_header[0],
            record_header[1],
            record_header[2],
            record_header[3],
        ]);
        let stored_id = record_header[4];
        if length < 1 {
            return Err(RegionError::Corrupt(format!(
                "chunk record declares length {length}"
            )));
        }
        let payload_length = u64::from(length.unsigned_abs()) - 1;
        let external = stored_id & EXTERNAL_FLAG != 0;
        let id = stored_id & !EXTERNAL_FLAG;
        let compression =
            Compression::from_id(id).ok_or(RegionError::UnsupportedCompression(id))?;

        let data = if external {
            self.read_external(local_x, local_z)?
        } else {
            if 4 + u64::from(length.unsigned_abs()) > sector_count * SECTOR_BYTES {
                return Err(RegionError::Corrupt(format!(
                    "chunk record of {length} bytes overruns its {sector_count} sectors"
                )));
            }
            if start + RECORD_HEADER_BYTES + payload_length > self.length {
                return Err(RegionError::Corrupt("chunk record is truncated".to_owned()));
            }
            let mut data = vec![0u8; payload_length as usize];
            self.file.read_exact(&mut data)?;
            data
        };
        Ok(Some(RawChunk {
            compression,
            data,
            timestamp: self.timestamps[slot],
        }))
    }

    /// Reads the oversized chunk's `c.<chunkX>.<chunkZ>.mcc` payload.
    fn read_external(&self, local_x: usize, local_z: usize) -> Result<Vec<u8>, RegionError> {
        let chunk_x = self.region_x * REGION_WIDTH as i32 + local_x as i32;
        let chunk_z = self.region_z * REGION_WIDTH as i32 + local_z as i32;
        let path = self.directory.join(format!("c.{chunk_x}.{chunk_z}.mcc"));
        let file = File::open(&path).map_err(|error| {
            RegionError::Corrupt(format!(
                "external chunk file {} is unreadable: {error}",
                path.display()
            ))
        })?;
        if file.metadata()?.len() > MAX_EXTERNAL_BYTES {
            return Err(RegionError::Corrupt(format!(
                "external chunk file {} is implausibly large",
                path.display()
            )));
        }
        let mut data = Vec::new();
        { file }.read_to_end(&mut data)?;
        Ok(data)
    }
}

/// Decompresses one chunk payload, refusing to expand past `limit` bytes.
///
/// # Errors
/// Returns [`RegionError::Corrupt`] for a malformed stream or an expansion past
/// `limit`.
pub(super) fn decompress(
    compression: Compression,
    data: &[u8],
    limit: usize,
) -> Result<Vec<u8>, RegionError> {
    match compression {
        Compression::Gzip => read_bounded(GzDecoder::new(data), limit),
        Compression::Zlib => read_bounded(ZlibDecoder::new(data), limit),
        Compression::None => {
            if data.len() > limit {
                return Err(too_large(limit));
            }
            Ok(data.to_vec())
        }
        Compression::Lz4 => decompress_lz4_blocks(data, limit),
    }
}

fn too_large(limit: usize) -> RegionError {
    RegionError::Corrupt(format!("chunk expands past {limit} bytes"))
}

fn read_bounded(reader: impl io::Read, limit: usize) -> Result<Vec<u8>, RegionError> {
    let mut output = Vec::new();
    // One byte past the limit tells "exactly at the limit" from "over it".
    let read = reader
        .take(limit as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|error| RegionError::Corrupt(format!("decompression failed: {error}")))?;
    if read > limit {
        return Err(too_large(limit));
    }
    Ok(output)
}

/// Decodes lz4-java's `LZ4BlockInputStream` framing, which vanilla writes for
/// compression id 4. The per-block checksum is not verified: the NBT parse that
/// follows rejects a damaged block.
fn decompress_lz4_blocks(data: &[u8], limit: usize) -> Result<Vec<u8>, RegionError> {
    let corrupt = |message: &str| RegionError::Corrupt(format!("LZ4 stream: {message}"));
    let mut output = Vec::new();
    let mut position = 0usize;
    while position < data.len() {
        let header = data
            .get(position..position + LZ4_BLOCK_HEADER_BYTES)
            .ok_or_else(|| corrupt("truncated block header"))?;
        if &header[..8] != LZ4_BLOCK_MAGIC {
            return Err(corrupt("bad block magic"));
        }
        let method = header[8] & LZ4_METHOD_MASK;
        let compressed_length = i32::from_le_bytes([header[9], header[10], header[11], header[12]]);
        let original_length = i32::from_le_bytes([header[13], header[14], header[15], header[16]]);
        position += LZ4_BLOCK_HEADER_BYTES;
        if original_length == 0 {
            // The empty block that closes the stream.
            return Ok(output);
        }
        let (Ok(compressed_length), Ok(original_length)) = (
            usize::try_from(compressed_length),
            usize::try_from(original_length),
        ) else {
            return Err(corrupt("negative block length"));
        };
        if output.len() + original_length > limit {
            return Err(too_large(limit));
        }
        let block = data
            .get(position..position + compressed_length)
            .ok_or_else(|| corrupt("truncated block"))?;
        position += compressed_length;
        match method {
            LZ4_METHOD_RAW => {
                if compressed_length != original_length {
                    return Err(corrupt("raw block length mismatch"));
                }
                output.extend_from_slice(block);
            }
            LZ4_METHOD_COMPRESSED => {
                let decoded = decompress_block(block, original_length)
                    .map_err(|error| corrupt(&error.to_string()))?;
                output.extend_from_slice(&decoded);
            }
            _ => return Err(corrupt("unknown block method")),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::{env, fs, process};

    use flate2::Compression as Level;
    use flate2::write::{GzEncoder, ZlibEncoder};

    use super::*;

    fn lz4_block(method: u8, compressed: &[u8], original_length: i32) -> Vec<u8> {
        let mut block = Vec::new();
        block.extend_from_slice(LZ4_BLOCK_MAGIC);
        block.push(method | 0x0F);
        block.extend_from_slice(&(compressed.len() as i32).to_le_bytes());
        block.extend_from_slice(&original_length.to_le_bytes());
        block.extend_from_slice(&0i32.to_le_bytes());
        block.extend_from_slice(compressed);
        block
    }

    #[test]
    fn every_compression_kind_round_trips() {
        let payload: Vec<u8> = (0..5000u32).map(|n| (n % 251) as u8).collect();

        let mut gzip = GzEncoder::new(Vec::new(), Level::default());
        gzip.write_all(&payload).expect("test setup");
        let mut zlib = ZlibEncoder::new(Vec::new(), Level::default());
        zlib.write_all(&payload).expect("test setup");

        // One raw block, one literal-only LZ4 block (token 0xF0 = 15 literals
        // plus a length byte), then the empty end block.
        let mut lz4 = lz4_block(LZ4_METHOD_RAW, &payload[..100], 100);
        let literals = &payload[100..130];
        let mut sequence = vec![0xF0, (literals.len() - 15) as u8];
        sequence.extend_from_slice(literals);
        lz4.extend(lz4_block(LZ4_METHOD_COMPRESSED, &sequence, 30));
        lz4.extend(lz4_block(LZ4_METHOD_RAW, &[], 0));

        let cases = [
            (
                Compression::Gzip,
                gzip.finish().expect("test setup"),
                payload.clone(),
            ),
            (
                Compression::Zlib,
                zlib.finish().expect("test setup"),
                payload.clone(),
            ),
            (Compression::None, payload.clone(), payload.clone()),
            (Compression::Lz4, lz4, payload[..130].to_vec()),
        ];
        for (kind, stored, expected) in cases {
            let decoded = decompress(kind, &stored, MAX_DECOMPRESSED_BYTES).expect("test setup");
            assert_eq!(decoded, expected, "{kind:?}");
        }
    }

    #[test]
    fn expansion_past_the_limit_is_refused() {
        let mut zlib = ZlibEncoder::new(Vec::new(), Level::best());
        zlib.write_all(&vec![0u8; 10_000]).expect("test setup");
        let stored = zlib.finish().expect("test setup");

        assert!(decompress(Compression::Zlib, &stored, 10_000).is_ok());
        assert!(matches!(
            decompress(Compression::Zlib, &stored, 9_999),
            Err(RegionError::Corrupt(_))
        ));
        assert!(matches!(
            decompress(Compression::Lz4, &lz4_block(LZ4_METHOD_RAW, &[1; 8], 8), 4),
            Err(RegionError::Corrupt(_))
        ));
    }

    #[test]
    fn a_truncated_lz4_block_is_corrupt_not_a_panic() {
        let mut block = lz4_block(LZ4_METHOD_RAW, &[7; 40], 40);
        block.truncate(block.len() - 5);
        assert!(matches!(
            decompress(Compression::Lz4, &block, MAX_DECOMPRESSED_BYTES),
            Err(RegionError::Corrupt(_))
        ));
    }

    #[test]
    fn unknown_compression_ids_are_named() {
        assert_eq!(Compression::from_id(4), Some(Compression::Lz4));
        assert_eq!(Compression::from_id(0), None);
        assert_eq!(Compression::from_id(5), None);
    }

    /// Writes a region whose only chunk sits at the given local coordinates.
    fn write_region(
        directory: &Path,
        name: &str,
        local: (usize, usize),
        stored_id: u8,
        payload: &[u8],
    ) -> PathBuf {
        let mut file = vec![0u8; HEADER_BYTES as usize];
        let sectors = (RECORD_HEADER_BYTES as usize + payload.len()).div_ceil(4096);
        let slot = local.1 * 32 + local.0;
        file[slot * 4..slot * 4 + 4].copy_from_slice(&((2u32 << 8) | sectors as u32).to_be_bytes());
        file[4096 + slot * 4..4096 + slot * 4 + 4].copy_from_slice(&1_700_000_000u32.to_be_bytes());
        file.extend_from_slice(&(payload.len() as i32 + 1).to_be_bytes());
        file.push(stored_id);
        file.extend_from_slice(payload);
        file.resize(HEADER_BYTES as usize + sectors * 4096, 0);
        let path = directory.join(name);
        fs::write(&path, file).expect("test setup");
        path
    }

    fn test_directory(name: &str) -> PathBuf {
        let directory = env::temp_dir().join(format!(
            "foton-anvil-{name}-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        fs::create_dir_all(&directory).expect("test setup");
        directory
    }

    #[test]
    fn an_inline_chunk_is_found_by_its_local_coordinates() {
        let directory = test_directory("inline");
        let path = write_region(&directory, "r.-1.2.mca", (5, 7), 3, b"hello");

        let mut region = AnvilRegion::open(&path).expect("test setup");
        assert_eq!((region.region_x(), region.region_z()), (-1, 2));
        assert!(region.contains(5, 7));
        assert!(!region.contains(7, 5));
        assert!(region.read_raw(7, 5).expect("test setup").is_none());

        let raw = region
            .read_raw(5, 7)
            .expect("test setup")
            .expect("test setup");
        assert_eq!(raw.compression, Compression::None);
        assert_eq!(raw.timestamp, 1_700_000_000);
        assert_eq!(raw.decompress().expect("test setup"), b"hello");
        fs::remove_dir_all(directory).expect("test setup");
    }

    #[test]
    fn an_external_chunk_is_read_from_its_mcc_file_by_absolute_coordinates() {
        let directory = test_directory("external");
        // Region (1, -1), local (2, 3): chunk (34, -29).
        let path = write_region(&directory, "r.1.-1.mca", (2, 3), 3 | EXTERNAL_FLAG, &[]);
        fs::write(directory.join("c.34.-29.mcc"), b"oversized").expect("test setup");

        let mut region = AnvilRegion::open(&path).expect("test setup");
        let raw = region
            .read_raw(2, 3)
            .expect("test setup")
            .expect("test setup");
        assert_eq!(raw.compression, Compression::None);
        assert_eq!(raw.decompress().expect("test setup"), b"oversized");

        fs::remove_file(directory.join("c.34.-29.mcc")).expect("test setup");
        assert!(matches!(
            AnvilRegion::open(&path).expect("test setup").read_raw(2, 3),
            Err(RegionError::Corrupt(_))
        ));
        fs::remove_dir_all(directory).expect("test setup");
    }

    #[test]
    fn a_record_that_overruns_its_sectors_is_corrupt() {
        let directory = test_directory("overrun");
        let path = write_region(&directory, "r.0.0.mca", (0, 0), 3, b"abcd");
        // Claim a length far larger than the one sector the table grants.
        let mut bytes = fs::read(&path).expect("test setup");
        bytes[8192..8196].copy_from_slice(&100_000i32.to_be_bytes());
        fs::write(&path, bytes).expect("test setup");

        assert!(matches!(
            AnvilRegion::open(&path).expect("test setup").read_raw(0, 0),
            Err(RegionError::Corrupt(_))
        ));
        fs::remove_dir_all(directory).expect("test setup");
    }

    #[test]
    fn a_header_only_region_reads_as_empty() {
        let directory = test_directory("empty");
        let path = directory.join("r.0.0.mca");
        fs::write(&path, []).expect("test setup");
        let region = AnvilRegion::open(&path).expect("test setup");
        assert!((0..32).all(|x| !region.contains(x, 0)));
        fs::remove_dir_all(directory).expect("test setup");
    }
}
