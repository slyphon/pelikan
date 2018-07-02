pub use self::errors::CDBError;
use bytes::BytesMut;
use bytes::{Buf, BufMut, Bytes, IntoBuf};
use failure;

use memmap::Mmap;
use std::cmp;
use std::fmt;
use std::io::SeekFrom;
use std::io::prelude::*;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::result;
use std::{fs, fs::File};

pub mod errors;
pub mod input;

pub const STARTING_HASH: u32 = 5381;
const MAIN_TABLE_SIZE: usize = 256;
const MAIN_TABLE_SIZE_BYTES: usize = 2048;
const END_TABLE_ENTRY_SIZE: usize = 8;
const INDEX_ENTRY_SIZE: usize = 8;

pub type Result<T> = result::Result<T, failure::Error>;

pub enum Source<'a> {
    Path(PathBuf),
    File(&'a mut fs::File),
}

impl<'a> From<PathBuf> for Source<'a> {
    fn from(pb: PathBuf) -> Self {
        Source::Path(pb)
    }
}

impl<'a> From<&'a mut fs::File> for Source<'a> {
    fn from(f: &'a mut File) -> Self {
        Source::File(f)
    }
}

// NOTE: this crosses the FFI boundary, so be careful with what you add to this
#[repr(C)]
pub enum CDBData {
    Boxed(Box<[u8]>),
    Mmapped(Mmap),
}

impl CDBData {
    pub fn new(source: Source, lopt: LoadOption) -> Result<CDBData> {
        match (source, lopt) {
            (Source::Path(pb), LoadOption::HEAP) => {
                Self::load_bytes_at_path(&pb).map(|b| CDBData::Boxed(b))
            }
            (Source::Path(pb), LoadOption::MMAP) => {
                Self::mmap_path(&pb).map(|m| CDBData::Mmapped(m))
            }
            (Source::File(ref mut f), LoadOption::HEAP) => {
                Self::load_bytes_from_file(f).map(|b| CDBData::Boxed(b))
            }
            (Source::File(ref mut f), LoadOption::MMAP) => {
                Self::mmap_file(f).map(|b| CDBData::Mmapped(b))
            }
        }
    }

    fn load_bytes_from_file(f: &mut File) -> Result<Box<[u8]>> {
        let mut buffer = Vec::with_capacity(f.metadata()?.len() as usize);
        f.read_to_end(&mut buffer)?;
        Ok(buffer.into_boxed_slice())
    }

    fn load_bytes_at_path(path: &Path) -> Result<Box<[u8]>> {
        let mut f = File::open(path)?;
        Self::load_bytes_from_file(&mut f)
    }

    fn mmap_path(path: &Path) -> Result<Mmap> {
        let f = File::open(path)?;
        Self::mmap_file(&f)
    }

    fn mmap_file(f: &File) -> Result<Mmap> {
        unsafe { Mmap::map(&f) }.map_err(|e| e.into())
    }
}

impl From<Mmap> for CDBData {
    fn from(m: Mmap) -> Self {
        CDBData::Mmapped(m)
    }
}

impl From<Box<[u8]>> for CDBData {
    fn from(b: Box<[u8]>) -> Self {
        CDBData::Boxed(b)
    }
}

impl From<Vec<u8>> for CDBData {
    fn from(v: Vec<u8>) -> Self {
        CDBData::from(v.into_boxed_slice())
    }
}

impl AsRef<[u8]> for CDBData {
    fn as_ref(&self) -> &[u8] {
        match self {
            CDBData::Mmapped(map) => &map[..],
            CDBData::Boxed(bx) => &bx[..],
        }
    }
}

#[repr(C)]
pub enum LoadOption {
    HEAP = 1,
    MMAP = 2,
}

// idea from https://raw.githubusercontent.com/jothan/cordoba/master/src/lib.rs
#[derive(Copy, Clone, Default, Eq, PartialEq)]
#[repr(C)]
pub(crate) struct CDBHash(u32);

impl CDBHash {
    fn new(bytes: &[u8]) -> Self {
        let mut h = STARTING_HASH;

        for b in bytes {
            // wrapping here is explicitly for allowing overflow semantics:
            //
            //   Operations like + on u32 values is intended to never overflow,
            //   and in some debug configurations overflow is detected and results in a panic.
            //   While most arithmetic falls into this category, some code explicitly expects
            //   and relies upon modular arithmetic (e.g., hashing)
            //
            h = h.wrapping_shl(5).wrapping_add(h) ^ (*b as u32)
        }
        CDBHash(h)
    }

    #[inline]
    fn table(&self) -> usize {
        self.0 as usize % MAIN_TABLE_SIZE
    }

    #[inline]
    fn slot(&self, num_ents: usize) -> usize {
        (self.0 as usize >> 8) % num_ents
    }

    #[inline]
    fn inner(&self) -> u32 {
        self.0
    }
}

impl fmt::Debug for CDBHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "CDBHash(0x{:08x})", self.0)
    }
}

impl<'a> From<&'a CDBHash> for usize {
    #[inline]
    fn from(h: &'a CDBHash) -> Self {
        h.0 as usize
    }
}

impl<'a> From<&'a CDBHash> for u32 {
    #[inline]
    fn from(h: &'a CDBHash) -> Self {
        h.0
    }
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub(crate) struct Bucket {
    ptr: u32,
    num_ents: u32,
}

impl fmt::Debug for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(
            f,
            "TableRec {{ ptr: {:>#010x}, num_ents: {:>#010x} }}",
            self.ptr, self.num_ents
        )
    }
}

impl Bucket {
    // returns the offset into the db of entry n of this bucket.
    // panics if n >= num_ents
    fn entry_n_pos<'a>(&'a self, n: usize) -> IndexEntryPos {
        assert!(n < self.num_ents as usize);
        IndexEntryPos(self.ptr as usize + (n * END_TABLE_ENTRY_SIZE))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
struct IndexEntryPos(usize);

impl From<IndexEntryPos> for usize {
    fn from(n: IndexEntryPos) -> Self {
        n.0
    }
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct KV {
    pub k: Bytes,
    pub v: Bytes,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct KVRef<'a> {
    pub k: &'a [u8],
    pub v: &'a [u8],
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub(crate) struct IndexEntry {
    hash: CDBHash, // the hash of the stored key
    ptr: u32,      // pointer to the absolute position of the data in the db
}

#[derive(Debug)]
#[repr(C)]
pub struct Reader<'a> {
    data: &'a [u8]
}

impl<'a> Reader<'a> {
    pub fn new<'b, T: AsRef<[u8]>>(r: &'b T) -> Reader<'b> {
        Reader { data: r.as_ref() }
    }

    #[inline]
    fn bucket_at(&self, idx: usize) -> Result<Bucket> {
        assert!(idx < MAIN_TABLE_SIZE);

        let off = 8 * idx;

        let slice = self.data[off..(off + 8)].as_ref();
        let b = slice.into_buf();
        assert_eq!(slice.len(), 8);
        trace!("bucket_at idx: {}, got buf: {:?}", idx, b);

        let mut buf = b.into_buf();

        let ptr = buf.get_u32_le();
        let num_ents = buf.get_u32_le();

        Ok(Bucket { ptr, num_ents })
    }

    // returns the index entry at absolute position 'pos' in the db
    #[inline]
    fn index_entry_at(&self, pos: IndexEntryPos) -> Result<IndexEntry> {
        let pos: usize = pos.into();

        if pos < MAIN_TABLE_SIZE_BYTES {
            panic!("position {:?} was in the main table!", pos)
        }

        let mut b = self.data[pos..(pos + 8)].into_buf();
        let hash = CDBHash(b.get_u32_le());
        let ptr = b.get_u32_le();

        Ok(IndexEntry { hash, ptr })
    }

    #[inline]
    fn get_kv_ref(&self, ie: IndexEntry) -> Result<KVRef<'a>> {
        let b = self.data[(ie.ptr as usize)..(ie.ptr as usize + INDEX_ENTRY_SIZE)].as_ref();

        let ksize = b[..4].into_buf().get_u32_le() as usize;
        let vsize = b[4..].into_buf().get_u32_le() as usize;

        let kstart = ie.ptr as usize + INDEX_ENTRY_SIZE;
        let vstart = kstart as usize + ksize;

        let k = &self.data[kstart..(kstart + ksize)];
        let v = &self.data[vstart..(vstart + vsize)];

        Ok(KVRef { k, v })
    }

    pub fn get(&self, key: &[u8], buf: &mut[u8]) -> Result<Option<usize>> {
        let key = key.into();
        let hash = CDBHash::new(key);
        let bucket = self.bucket_at(hash.table())?;

        if bucket.num_ents == 0 {
            trace!("bucket empty, returning none");
            return Ok(None);
        }

        let slot = hash.slot(bucket.num_ents as usize);

        for x in 0..bucket.num_ents {
            let index_entry_pos =
                bucket.entry_n_pos(((x + slot as u32) % bucket.num_ents) as usize);

            let idx_ent = self.index_entry_at(index_entry_pos)?;

            if idx_ent.ptr == 0 {
                return Ok(None);
            } else if idx_ent.hash == hash {
                let kv = self.get_kv_ref(idx_ent)?;
                if &kv.k[..] == key {
                    return Ok(Some(copy_slice(buf, kv.v)));
                } else {
                    continue;
                }
            }
        }

        Ok(None)
    }
}

#[inline]
fn copy_slice(dst: &mut [u8], src: &[u8]) -> usize {
    let n = cmp::min(dst.len(), src.len());
    dst[0..n].copy_from_slice(&src[0..n]);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_temp_cdb<'a>(kvs: &Vec<(String, String)>) -> Result<Box<[u8]>> {
        let mut ntf = NamedTempFile::new()?;

        {
            let mut w = Writer::new(ntf.as_file_mut())?;
            for kv in kvs {
                let (k, v) = kv.clone();
                w.put(&k.into_bytes(), &v.into_bytes())?;
            }
        }

        let mut buf = Vec::new();
        ntf.read_to_end(&mut buf)?;
        Ok(buf.into_boxed_slice())
    }


    #[test]
    fn round_trip_test() {
        let kvs: Vec<(String, String)> = vec![
            ("abc", "def"),
            ("pink", "red"),
            ("apple", "grape"),
            ("q", "burp"),
        ].iter()
            .map(|(k,v)| (k.to_string(), v.to_string()))
            .collect();

        let data = create_temp_cdb(&kvs).unwrap();

        let cdb = Reader { data: &data };

        for (k, v) in kvs {
            let mut buf = Vec::new();
            buf.resize(10, 0u8);

            let n = cdb.get(k.as_bytes(), &mut buf[..]).unwrap().unwrap();
            assert_eq!(n, v.len());
            assert_eq!(&buf[0..n], v.as_bytes())
        }

        let mut buf = Vec::new();
        buf.resize(10, 0u8);

        let r = cdb.get("1233".as_bytes(), &mut buf[..]).unwrap();
        assert!(r.is_none());
    }
}

fn ready_buf(size: usize) -> BytesMut {
    let mut b = BytesMut::with_capacity(size);
    b.reserve(size);
    b
}

pub struct Writer<'a, F>
where
    F: Write + Seek + 'a,
{
    file: &'a mut F,
    index: Vec<Vec<IndexEntry>>,
}

impl<'a, F> Writer<'a, F>
where
    F: Write + Seek + 'a,
{
    pub fn new(file: &'a mut F) -> Result<Writer<'a, F>> {
        file.seek(SeekFrom::Start(0))?;
        file.write(&[0u8; MAIN_TABLE_SIZE_BYTES])?;

        Ok(Writer {
            file,
            index: vec![vec![IndexEntry::default()]; 256],
        })
    }

    fn seek(&mut self, sf: SeekFrom) -> Result<u32> {
        self.file.seek(sf).map(|n| n as u32).map_err(|e| e.into())
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        let ptr = self.seek(SeekFrom::Current(0))?;
        let mut buf = ready_buf(INDEX_ENTRY_SIZE + key.len() + value.len());

        buf.put_u32_le(key.len() as u32);
        buf.put_u32_le(value.len() as u32);
        buf.extend_from_slice(key);
        buf.extend_from_slice(value);

        self.file.write_all(&buf[..])?;

        let hash = CDBHash::new(key);
        self.index[hash.table() as usize].push(IndexEntry { hash, ptr });
        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        let mut buckets: Vec<Bucket> = Vec::with_capacity(256);
        self.file.seek(SeekFrom::End(0))?;

        let idx = self.index.clone();

        for tbl in idx {
            let length = (tbl.len() << 1) as u32;
            let mut ordered: Vec<IndexEntry> = vec![IndexEntry::default(); length as usize];
            for idx_ent in tbl {
                let slot = idx_ent.hash.slot(length as usize);
                for i in 0..length {
                    let j = (i + slot as u32) % length;
                    if ordered[j as usize].ptr == 0 {
                        ordered[j as usize] = idx_ent.clone();
                        break;
                    }
                }
            }

            // move to EOF and write out the secondary index entries, constructing the
            // primary table as we go ('buckets')
            //
            buckets.push(Bucket {
                ptr: self.seek(SeekFrom::End(0))?,
                num_ents: length,
            });

            let mut buf = ready_buf((ordered.len() * 8) as usize);

            for idx_ent in ordered {
                buf.put_u32_le(idx_ent.hash.inner());
                buf.put_u32_le(idx_ent.ptr);
            }

            self.file.write_all(&buf[..])?;
        }

        // now write the buckets
        //
        self.file.seek(SeekFrom::Start(0))?;
        {
            let mut buf = ready_buf(2048);

            for bkt in buckets {
                buf.put_u32_le(bkt.ptr);
                buf.put_u32_le(bkt.num_ents);
            }

            self.file.write_all(&buf[..])?;
        }

        // start at BOF
        self.file.seek(SeekFrom::Start(0))?;

        Ok(())
    }
}

impl<'a, F> Drop for Writer<'a, F>
where
    F: Write + Seek + 'a,
{
    fn drop(&mut self) {
        self.finalize().unwrap();
    }
}
