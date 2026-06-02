#[cfg(test)]
mod test;

use bitflags::bitflags;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use byteorder_pack::UnpackFrom;
use std::{
    cell::RefCell,
    fs::{self, File},
    io::{self, BufWriter, Read, Seek, Write},
    ops::BitAnd,
    path::{Path, PathBuf},
    rc::Rc,
};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, VFFError>;

const FAT16_MAX_CLUSTERS: u32 = 0xfff5;
const FAT12_MAX_CLUSTERS: u32 = 0xff5;
const EXPECTED_FILE_MAGIC: [u8; 4] = [b'V', b'F', b'F', b' '];

#[derive(Error, Debug)]
pub enum VFFError {
    #[error("IO Error: {0}")]
    IOErr(#[from] io::Error),
    #[error("Error: {0}")]
    Other(String),
    #[error("invalid data in {context}: (expected {expected}, found {found})")]
    InvalidData {
        context: String,
        expected: String,
        found: String,
    },
}

#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SupportedFAT {
    FAT16,
}

impl SupportedFAT {
    fn get_reserved_marker(&self) -> u32 {
        match self {
            Self::FAT16 => 0xfff0,
        }
    }
    fn mask(&self, input: u32) -> usize {
        match self {
            Self::FAT16 => (input & 0xffff) as usize,
        }
    }
}

#[derive(Debug)]
pub struct FAT {
    fattype: SupportedFAT,
    clusters: Vec<u16>,
}

impl FAT {
    pub fn new(fd: &mut impl Read, header: &VFFHeader) -> Result<Self> {
        let cluster_count = header.cluster_count;
        let cluster_size = header.cluster_size as u32;
        let fattype: SupportedFAT;
        let fatsize: u32;
        if cluster_count > FAT16_MAX_CLUSTERS {
            return Err(VFFError::Other("FAT 32 is not supported".to_owned()));
        }
        if cluster_count > FAT12_MAX_CLUSTERS {
            fattype = SupportedFAT::FAT16;
            fatsize = cluster_count * 2;
        } else {
            return Err(VFFError::Other("FAT12 is not supported".to_owned()));
        }
        let buf_size = (fatsize + cluster_size - 1) & !(cluster_size - 1);
        let mut clusters = Vec::with_capacity(buf_size as usize);
        clusters.resize_with(buf_size as usize, Default::default);
        fd.read_u16_into::<LittleEndian>(clusters.as_mut_slice())?;
        Ok(Self { fattype, clusters })
    }

    fn get_fat16(&self, index: usize) -> Result<u32> {
        if self.fattype != SupportedFAT::FAT16 {
            return Err(VFFError::Other(
                "This function should only be called for FAT16".to_owned(),
            ));
        }
        if let Some(res) = self.clusters.get(index) {
            Ok(*res as u32)
        } else {
            let expected = "Indexing into the cluster data at a valid location".to_owned();
            let found = format!("Cluster data wasn't long enough to index that far. Asked for: {index} Cluster len: {}", self.clusters.len());
            Err(VFFError::InvalidData {
                context: "get_cluster FAT16".to_owned(),
                expected,
                found,
            })
        }
    }

    pub fn get_cluster(&self, index: u32) -> Result<u32> {
        let index = self.fattype.mask(index);
        #[allow(unreachable_patterns)]
        match self.fattype {
            SupportedFAT::FAT16 => Ok(self.get_fat16(index)?),
            _ => Err(VFFError::Other("FAT type not supported".to_owned())),
        }
    }

    pub fn is_available(x: u32) -> bool {
        x == 0
    }

    pub fn is_used(&self, x: u32) -> bool {
        let reserved = self.fattype.get_reserved_marker();
        0x1 <= x && x < reserved
    }
    pub fn is_bad(&self, x: u32) -> bool {
        x == self.fattype.get_reserved_marker() + 7
    }
    pub fn is_last(&self, x: u32) -> bool {
        self.fattype.get_reserved_marker() + 8 <= x
    }
    pub fn get_chain(&self, start: u32) -> Result<Vec<u32>> {
        let mut chain: Vec<u32> = Vec::new();
        let mut current = start;
        while self.is_used(current) {
            chain.push(current);
            current = self.get_cluster(current)?;
        }
        if !self.is_last(current) {
            return Err(VFFError::InvalidData {
                context: "FAT chain parsing".to_owned(),
                expected: "The first unused cluster in the chain should satisfy is_last".to_owned(),
                found: format!("False, the cluster reads: {current:04x}"),
            });
        }
        Ok(chain)
    }
}

#[derive(Debug)]
pub struct VFFHeader {
    pub volume_size: u32,
    pub cluster_size: u16,
    pub cluster_count: u32,
}

fn check_header(vff_header: [u8; 0x10]) -> Result<VFFHeader> {
    let mut cursor = std::io::Cursor::new(vff_header);
    let (magic, _unknown_header_entry, volume_size, cluster_size) =
        <([u8; 4], u32, u32, u16)>::unpack_from_be(&mut cursor)?;
    let cluster_size = cluster_size.checked_mul(16).ok_or_else(|| {
        return VFFError::InvalidData {
            context: "Checking VFF Header - Compute cluster size".to_owned(),
            expected: "cluster_size * 16 should not overflow".to_owned(),
            found: "Overflow detected".to_owned(),
        };
    })?;
    if cluster_size == 0 {
        return Err(VFFError::InvalidData {
            context: "Check VFF Header".to_owned(),
            expected: "Cluster size != 0".to_owned(),
            found: "0".to_owned(),
        });
    }
    if magic != EXPECTED_FILE_MAGIC {
        return Err(VFFError::InvalidData {
            context: "Check VFF Header: parsing file magic".to_owned(),
            expected: format!("{EXPECTED_FILE_MAGIC:?}"),
            found: format!("{magic:?}"),
        });
    }
    Ok(VFFHeader {
        volume_size,
        cluster_size,
        cluster_count: volume_size / cluster_size as u32,
    })
}

bitflags! {
    struct DirectoryFlags: u8 {
        const A_R   =  1;
        const A_H   =  2;
        const A_S   =  4;
        const A_VL  =  8;
        const A_DIR = 16;
        const A_A   = 32;
        const A_DEV = 64;
    }
}

impl BitAnd<DirectoryFlags> for u8 {
    type Output = u8;

    fn bitand(self, rhs: DirectoryFlags) -> Self::Output {
        rhs.bits() & self
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct ParsedFATEntry {
    pub name: [u8; 8],
    pub ext: [u8; 3],
    pub attr: u8,
    pub rsv: u8,
    pub cms: u8,
    pub ctime: u16,
    pub cdate: u16,
    pub adate: u16,
    pub eaindex: u16,
    pub mtime: u16,
    pub mdate: u16,
    pub start: u16,
    pub size: u32,
    pub deleted: bool,
}

impl ParsedFATEntry {
    pub fn from_slice(data: &mut [u8; 32]) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(data);
        let (name, ext) = <([u8; 8], [u8; 3])>::unpack_from_le(&mut cursor)?;
        let (attr, rsv, cms) = <(u8, u8, u8)>::unpack_from_le(&mut cursor)?;
        let (ctime, cdate, adate) = <(u16, u16, u16)>::unpack_from_le(&mut cursor)?;
        let (eaindex, mtime, mdate) = <(u16, u16, u16)>::unpack_from_le(&mut cursor)?;
        let (start, size) = <(u16, u32)>::unpack_from_le(&mut cursor)?;
        Ok(ParsedFATEntry {
            name,
            ext,
            attr,
            rsv,
            cms,
            ctime,
            cdate,
            adate,
            eaindex,
            mtime,
            mdate,
            start,
            size,
            deleted: false,
        })
    }
    pub fn nice_name(&self) -> String {
        String::from_utf8_lossy(&self.name).trim_end().to_owned()
    }
    pub fn nice_extension(&self) -> String {
        String::from_utf8_lossy(&self.ext).trim_end().to_owned()
    }
    pub fn nice_full_name(&self) -> String {
        if self.attr & DirectoryFlags::A_DIR != 0 {
            return self.nice_name();
        }
        self.nice_name() + "." + &self.nice_extension()
    }
}

#[derive(Debug, Clone)]
pub enum DirectoryContent {
    Dir(Directory),
    File(Vec<u8>),
    NoContent,
}

#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    path: String,
    name: String,
    content: DirectoryContent,
}

impl DirectoryEntry {
    pub fn make_dir_entry(path: String, name: String, dir: Directory) -> Self {
        DirectoryEntry {
            path,
            name,
            content: DirectoryContent::Dir(dir),
        }
    }
    pub fn make_file_entry(path: String, name: String, file: Vec<u8>) -> Self {
        DirectoryEntry {
            path,
            name,
            content: DirectoryContent::File(file),
        }
    }
    pub fn make_empty_file_entry(path: String, name: String) -> Self {
        Self::make_file_entry(path, name, Vec::with_capacity(0))
    }
    pub fn make_no_content(path: String) -> Self {
        DirectoryEntry {
            path,
            name: String::with_capacity(0),
            content: DirectoryContent::NoContent,
        }
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn file(&self) -> Option<&Vec<u8>> {
        match &self.content {
            DirectoryContent::File(f) => Some(f),
            _ => None,
        }
    }
    pub fn dir(&self) -> Option<&Directory> {
        match &self.content {
            DirectoryContent::Dir(d) => Some(d),
            _ => None,
        }
    }
    pub fn content(&self) -> &DirectoryContent {
        &self.content
    }
}

#[derive(Debug, Clone)]
pub struct Directory {
    vff: Rc<RefCell<VFF>>,
    data: Vec<u8>,
    path: String,
}

impl Directory {
    pub fn new(vff: Rc<RefCell<VFF>>, data: Vec<u8>, path: String) -> Result<Self> {
        let data_len = data.len();
        if data_len % 32 != 0 {
            return Err(VFFError::InvalidData {
                context: "Directory::new".to_owned(),
                expected: "Construct directory with a multiple of 32 bytes".to_owned(),
                found: format!("Constructed with {data_len} (not multiple of 32"),
            });
        }
        Ok(Directory { vff, data, path })
    }
    fn read(&self, show_deleted: bool) -> Result<Vec<ParsedFATEntry>> {
        let mut files: Vec<ParsedFATEntry> = Vec::new();
        for chunk in self.data.chunks_exact(32) {
            let mut chunk = <[u8; 32]>::try_from(chunk).unwrap(); // Won't panic because we got our slice from chunks_exact
            let mut parsed_entry = ParsedFATEntry::from_slice(&mut chunk)?;
            match parsed_entry.name[0] {
                0x0 => {
                    continue;
                } //free entry marker
                0xe5 => {
                    //deleted entry marker
                    if !show_deleted {
                        continue;
                    }
                    parsed_entry.deleted = true;
                }
                _ => {}
            }
            if parsed_entry.attr & 0xf == 0xf {
                continue;
            }
            files.push(parsed_entry);
        }
        Ok(files)
    }

    fn get(&self, name: String, show_deleted: bool) -> Result<DirectoryEntry> {
        for entry in self.read(show_deleted)? {
            let entry_name = entry.nice_name();
            if entry_name.to_ascii_lowercase() == name.to_ascii_lowercase() {
                // Match!
                if entry.attr & DirectoryFlags::A_DIR != 0 {
                    // It's a directory
                    let new_data = self.vff.borrow_mut().read_chain(entry.start.into())?;
                    let path = self.path.clone() + "/" + &entry_name;
                    return Ok(DirectoryEntry::make_dir_entry(
                        self.path.clone(),
                        entry_name,
                        Directory::new(self.vff.clone(), new_data, path)?,
                    ));
                } else if entry.size == 0 {
                    // It's an empty file
                    return Ok(DirectoryEntry::make_empty_file_entry(
                        self.path.clone(),
                        entry_name,
                    ));
                } else {
                    let mut vff = self.vff.borrow_mut();
                    let mut raw = vff.read_chain(entry.start.into())?;
                    raw.truncate(entry.size as usize);
                    drop(vff);

                    return Ok(DirectoryEntry::make_file_entry(
                        self.path.clone(),
                        entry_name,
                        raw,
                    ));
                }
            }
        }
        Ok(DirectoryEntry::make_no_content(self.path.clone()))
    }

    pub fn ls(&self, include_deleted: bool) -> Result<Vec<String>> {
        self.do_operation_recursive(None, include_deleted)
    }

    pub fn dump(&self, dump_location: PathBuf, include_deleted: bool) -> Result<()> {
        std::fs::create_dir_all(&dump_location)?;
        self.do_operation_recursive(Some(dump_location), include_deleted)?;
        Ok(())
    }

    fn do_operation_recursive(
        &self,
        dump: Option<PathBuf>,
        show_deleted: bool,
    ) -> Result<Vec<String>> {
        let mut res: Vec<String> = Vec::new();
        // We need to make sure our directory gets added if it's empty
        // we can't just check the length of the result from read(), because we might filter out a result like '.' or '..'
        let mut got_ourself = false;
        for entry in self.read(show_deleted)? {
            if entry.attr & DirectoryFlags::A_DIR != 0 {
                match entry.nice_name().as_ref() {
                    "." | ".." => continue,
                    _ => {
                        got_ourself = true;
                    }
                }
                let maybe_error = "Directory::get should return another Directory because the entry is marked as one in the FAT".to_owned();
                #[allow(unused_assignments)]
                let mut maybe_found = "Placeholder error text";
                match self.get(entry.nice_name(), show_deleted)?.content {
                    DirectoryContent::Dir(dir) => {
                        let new_dump = match &dump {
                            Some(path) => {
                                let mut temp = path.to_owned();
                                temp.push(&entry.nice_name());
                                std::fs::create_dir_all(path)?;
                                Some(temp)
                            }
                            None => None,
                        };
                        let directory_recused =
                            dir.do_operation_recursive(new_dump, show_deleted)?;
                        res.extend(directory_recused);
                        continue;
                    }
                    DirectoryContent::File(_) => {
                        maybe_found = "returned file contents";
                    }
                    DirectoryContent::NoContent => {
                        maybe_found = "returned nothing";
                    }
                }
                return Err(VFFError::InvalidData {
                    context: "Directory::ls get entry from read".to_owned(),
                    expected: maybe_error,
                    found: maybe_found.to_owned(),
                });
            } else if let Some(path) = &dump {
                got_ourself = true;
                if let DirectoryContent::File(file_bytes) =
                    self.get(entry.nice_name(), show_deleted)?.content()
                {
                    std::fs::create_dir_all(path)?;
                    let mut temp = path.to_owned();
                    temp.push(&entry.nice_full_name());
                    let mut f = BufWriter::new(File::create(temp)?);
                    f.write_all(file_bytes.as_slice())?;
                } else {
                    return Err(VFFError::InvalidData {
                        context: "Directory::ls dumping file get".to_owned(),
                        expected: "Directory::get returns file bytes".to_owned(),
                        found: "None".to_owned(),
                    });
                }
            } else {
                got_ourself = true;
                let mut final_name = self.path.clone()
                    + "/"
                    + &entry.nice_full_name()
                    + &format!(" [{:#06x}]", entry.size);
                if entry.deleted {
                    final_name += " [DELETED]"
                }
                res.push(final_name);
            }
        }
        if !got_ourself {
            res.push(self.path.to_owned());
        }
        Ok(res)
    }
}

trait ReadSeek: Read + Seek + std::fmt::Debug {}
impl<T> ReadSeek for T where T: Read + Seek + std::fmt::Debug {}

#[derive(Debug)]
pub struct VFF {
    fd: Box<dyn ReadSeek>,
    header: VFFHeader,
    parsed_fat1: FAT,
    data_offset: u64,
}

impl VFF {
    pub fn new<T: Read+Seek+std::fmt::Debug+'static>(fd: T) -> Result<(Rc<RefCell<Self>>, Directory)> {
        let mut fd: Box<dyn ReadSeek> = Box::new(fd);
        let mut header = [0u8; 0x10];
        fd.read_exact(&mut header)?;
        fd.seek(io::SeekFrom::Current(0x10))?; // Seek an aditional 0x10
        let header = check_header(header)?;
        let parsed_fat1 = FAT::new(&mut fd, &header)?;
        let mut root_data = Vec::with_capacity(0x1000);
        root_data.resize_with(0x1000, Default::default);
        fd.read_exact(root_data.as_mut_slice())?;
        let data_offset = fd.stream_position()?;

        let ret = Rc::new(RefCell::new(VFF {
            fd,
            header,
            parsed_fat1,
            data_offset,
        }));
        let root = Directory::new(ret.clone(), root_data, String::with_capacity(0))?;
        Ok((ret, root))
    }

    fn inner_read(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut ret: Vec<u8> = Vec::with_capacity(len);
        ret.resize_with(len, Default::default);
        self.fd.read_exact(ret.as_mut_slice())?;
        Ok(ret)
    }

    pub fn read_cluster(&mut self, cluster_num: u32) -> Result<Vec<u8>> {
        let cluster_num = cluster_num - 2;
        let offset = self.data_offset + self.header.cluster_size as u64 * cluster_num as u64;
        self.fd.seek(io::SeekFrom::Start(offset))?;
        self.inner_read(self.header.cluster_size as usize)
    }

    pub fn read_chain(&mut self, start: u32) -> Result<Vec<u8>> {
        let clusters = self.parsed_fat1.get_chain(start)?;
        let mut ret: Vec<u8> = Vec::new();
        for cluster in clusters {
            ret.extend(self.read_cluster(cluster)?);
        }
        Ok(ret)
    }
}

// Copied from typical Wii values. todo: ck if Wii software can deviate from this at all
pub const BUILD_CLUSTER_SIZE: u16 = 0x200;
pub const BUILD_DEFAULT_VOLUME_SIZE: u32 = 0x0140_0000;
const ROOT_DIR_CAPACITY: usize = 0x1000 / 32;

fn encode_83_name(filename: &str) -> Result<([u8; 8], [u8; 3])> {
    let upper = filename.to_ascii_uppercase();
    let (stem, ext): (&str, &str) = match upper.rfind('.') {
        Some(dot) => (&upper[..dot], &upper[dot + 1..]),
        None => (upper.as_str(), ""),
    };
    if stem.len() > 8 {
        return Err(VFFError::Other(format!(
            "Filename stem '{stem}' is longer than 8 characters and cannot be stored in 8.3 format"
        )));
    }
    if ext.len() > 3 {
        return Err(VFFError::Other(format!(
            "Filename extension '{ext}' is longer than 3 characters and cannot be stored in 8.3 format"
        )));
    }
    let mut name8 = [b' '; 8];
    let mut ext3 = [b' '; 3];
    name8[..stem.len()].copy_from_slice(stem.as_bytes());
    ext3[..ext.len()].copy_from_slice(ext.as_bytes());
    Ok((name8, ext3))
}

fn encode_dir_name(dirname: &str) -> Result<[u8; 8]> {
    let upper = dirname.to_ascii_uppercase();
    if upper.len() > 8 {
        return Err(VFFError::Other(format!(
            "Directory name '{upper}' is longer than 8 characters and cannot be stored in 8.3 format"
        )));
    }
    let mut name8 = [b' '; 8];
    name8[..upper.len()].copy_from_slice(upper.as_bytes());
    Ok(name8)
}

/// Write a single 32-byte FAT directory entry into `buf[offset..offset+32]`.
fn write_dir_entry( buf: &mut Vec<u8>, name8: [u8; 8], ext3: [u8; 3], attr: u8, start_cluster: u16, size: u32) {
    // todo copy fs metadata
    buf.extend_from_slice(&name8);   // fn
    buf.extend_from_slice(&ext3);    // ext
    buf.push(attr);                  // attr
    buf.push(0);                     // rsv
    buf.push(0);                     // cms
    buf.extend_from_slice(&[0, 0]);  // ctime
    buf.extend_from_slice(&[0, 0]);  // cdate
    buf.extend_from_slice(&[0, 0]);  // adate
    buf.extend_from_slice(&[0, 0]);  // eaindex
    buf.extend_from_slice(&[0, 0]);  // mtime
    buf.extend_from_slice(&[0, 0]);  // mdate
    // start cluster – le u16
    buf.push((start_cluster & 0xff) as u8);
    buf.push((start_cluster >> 8) as u8);
    // size – le u32
    buf.push((size & 0xff) as u8);
    buf.push(((size >> 8) & 0xff) as u8);
    buf.push(((size >> 16) & 0xff) as u8);
    buf.push(((size >> 24) & 0xff) as u8);
}

/// Allocate consecutive clusters starting at `next_free`, chaining them
/// in `fat`, and return the index of the first cluster in the chain.
/// After the call `next_free` points to the first unused cluster after the chain.
fn alloc_chain16(fat: &mut Vec<u16>, next_free: &mut u16, n_clusters: usize) -> Result<u16> {
    if n_clusters == 0 {
        // empty so no cluster allocated. callers store 0 as start_cluster for size==0.
        return Ok(0);
    }
    let start = *next_free;
    for i in 0..n_clusters {
        let cluster = *next_free;
        if cluster as usize >= fat.len() {
            return Err(VFFError::Other(
                "Not enough space in the VFF volume to pack all files".to_owned(),
            ));
        }
        if i + 1 == n_clusters {
            fat[cluster as usize] = 0xffff; // end-of-chain marker
        } else {
            fat[cluster as usize] = cluster + 1; // point to next
        }
        *next_free += 1;
    }
    Ok(start)
}

enum BuildFSEntry {
    File {
        name8: [u8; 8],
        ext3: [u8; 3],
        data: Vec<u8>,
    },
    Dir {
        name8: [u8; 8],
        children: Vec<BuildFSEntry>,
    },
}

// todo fix all the recursion in this project
fn scan_directory(src_dir: &Path) -> Result<Vec<BuildFSEntry>> {
    let mut entries: Vec<BuildFSEntry> = Vec::new();

    let mut dir_entries: Vec<_> = fs::read_dir(src_dir)
        .map_err(|e| VFFError::IOErr(e))?
        .filter_map(|e| e.ok())
        .collect();
    // do we need deterministic output?
    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            let name8 = encode_dir_name(&name_str)?;
            // recurse
            let children = scan_directory(&path)?;
            entries.push(BuildFSEntry::Dir { name8, children });
        } else if ft.is_file() {
            let (name8, ext3) = encode_83_name(&name_str)?;
            let data = fs::read(&path)?;
            entries.push(BuildFSEntry::File { name8, ext3, data });
        }
        // Symlinks and anything else are silently skipped.
    }
    Ok(entries)
}


/// FAT 16 only - todo genericize to `SupportedFAT`
struct Build16Ctx {
    fat: Vec<u16>,
    idx_free: u16,
    clusters: Vec<Vec<u8>>,
    cluster_size: usize,
}

impl Build16Ctx {
    fn new(cluster_count: u32) -> Self {
        let mut fat = vec![0u16; cluster_count as usize];
        // Reserved entries
        fat[0] = 0xfff8;
        fat[1] = 0xffff;
        Build16Ctx {
            fat,
            idx_free: 2,
            clusters: Vec::new(),
            cluster_size: BUILD_CLUSTER_SIZE as usize,
        }
    }

    /// Returns the first cluster index. 0 == emptry
    fn store_data(&mut self, data: &[u8]) -> Result<u16> {
        if data.is_empty() {
            return Ok(0);
        }
        let n = (data.len() + self.cluster_size - 1) / self.cluster_size;
        let start = alloc_chain16(&mut self.fat, &mut self.idx_free, n)?;
        let mut remaining = data;
        for _ in 0..n {
            let take = remaining.len().min(self.cluster_size);
            let mut cluster = vec![0u8; self.cluster_size];
            cluster[..take].copy_from_slice(&remaining[..take]);
            self.clusters.push(cluster);
            remaining = &remaining[take..];
        }
        Ok(start)
    }

    /// Returns (start_cluster, dir_bytes_for_dirent_in_parent)
    fn pack_dir_entries(&mut self, entries: &[BuildFSEntry]) -> Result<(u16, Vec<u8>)> {
        // First pass: recurse into children so we know their start clusters
        // before we serialise this directory's entries.
        let mut dirent_buf: Vec<u8> = Vec::new();

        for entry in entries {
            match entry {
                BuildFSEntry::File { name8, ext3, data } => {
                    let size = data.len() as u32;
                    let start = self.store_data(data)?;
                    write_dir_entry(&mut dirent_buf, *name8, *ext3, 0x20, start, size);
                }
                BuildFSEntry::Dir { name8, children } => {
                    let (child_start, child_dir_data) = self.pack_dir_entries(children)?;
                    let dir_start = self.store_data(&child_dir_data)?;
                    // The cluster chain start recorded in the parent is where
                    // the child directory data lives.
                    let _ = child_start; // (unused – dir_start is the real handle)
                    write_dir_entry(
                        &mut dirent_buf,
                        *name8,
                        [b' '; 3],
                        0x10, // A_DIR
                        dir_start,
                        0,
                    );
                }
            }
        }

        // The directory data block must be a multiple of 32 bytes; it already
        // is because each write_dir_entry emits exactly 32 bytes.  We do NOT
        // store it here – the caller decides whether this is the root (stored
        // separately) or a sub-directory (stored via store_data).
        Ok((0, dirent_buf))
    }
}


pub fn build_vff(src_dir: &Path, dest: &Path, volume_size: u32) -> Result<()> {
    let cluster_size = BUILD_CLUSTER_SIZE as u32;
    if volume_size % cluster_size != 0 {
        return Err(VFFError::Other(format!(
            "volume_size ({volume_size:#x}) must be a multiple of cluster_size ({cluster_size:#x})"
        )));
    }
    let cluster_count = volume_size / cluster_size;
    if cluster_count <= FAT12_MAX_CLUSTERS {
        return Err(VFFError::Other(
            "Requested volume size would require FAT12, which is not supported for packing"
                .to_owned(),
        ));
    }
    if cluster_count > FAT16_MAX_CLUSTERS {
        return Err(VFFError::Other(
            "Requested volume size requires FAT32, which is not supported".to_owned(),
        ));
    }

    // copy of WC24_GetVFF_FATSize
    // FAT16: fat_size = cluster_count * 2 bytes, aligned up to cluster_size.
    let fat_size_bytes: u32 = {
        let raw = cluster_count * 2;
        (raw + cluster_size - 1) & !(cluster_size - 1)
    };
    let root_entries = scan_directory(src_dir)?;
    let mut ctx = Build16Ctx::new(cluster_count);
    let (_, root_dir_data) = ctx.pack_dir_entries(&root_entries)?;

    if root_dir_data.len() > ROOT_DIR_CAPACITY * 32 {
        return Err(VFFError::Other(format!(
            "Root directory has {} entries, but only {} fit in a VFF root directory",
            root_dir_data.len() / 32,
            ROOT_DIR_CAPACITY
        )));
    }
    // fat_size_bytes / 2 = number of u16 entries to emit.
    let fat_entries = (fat_size_bytes / 2) as usize;
    let mut fat_bytes: Vec<u8> = Vec::with_capacity(fat_size_bytes as usize);
    for i in 0..fat_entries {
        let val = if i < ctx.fat.len() { ctx.fat[i] } else { 0u16 };
        fat_bytes.write_u16::<LittleEndian>(val)?;
    }
    // FAT2 is an identical copy of FAT1.
    let fat2_bytes = fat_bytes.clone();


    // here we go
    let out_file = File::create(dest)?;
    let mut w = BufWriter::new(out_file);
    // VFF header (big-endian, 0x20 bytes total)
    // Bytes 0-3: magic 'VFF '
    w.write_all(&EXPECTED_FILE_MAGIC)?;
    // Bytes 4-5: byte-order marker (big-endian 0xFEFF)
    w.write_u16::<BigEndian>(0xFEFF)?;
    // Bytes 6-7: length field 0x0100 (?)
    w.write_u16::<BigEndian>(0x0100)?;
    // Bytes 8-11: total volume size (big-endian u32)
    w.write_u32::<BigEndian>(volume_size)?;
    // Bytes 12-13: cluster_size / 16 (big-endian u16) – the raw header field
    let cluster_size_field = (BUILD_CLUSTER_SIZE / 16) as u16;
    w.write_u16::<BigEndian>(cluster_size_field)?;
    // Bytes 14-31: padding
    w.write_all(&[0u8; 18])?;
    // FAT1
    w.write_all(&fat_bytes)?;
    // FAT2 (redundant copy)
    w.write_all(&fat2_bytes)?;
    // Root directory (fixed 0x1000 bytes)
    let mut root_dir_padded = vec![0u8; 0x1000];
    let copy_len = root_dir_data.len().min(0x1000);
    root_dir_padded[..copy_len].copy_from_slice(&root_dir_data[..copy_len]);
    w.write_all(&root_dir_padded)?;
    // Data clusters
    for cluster in &ctx.clusters {
        w.write_all(cluster)?;
    }

    // calculate file padding
    let header_size: u64 = 0x20;
    let fat_total: u64 = fat_size_bytes as u64 * 2; // two fats hun
    let root_dir_size: u64 = 0x1000;
    let cluster_data_written: u64 = ctx.clusters.len() as u64 * BUILD_CLUSTER_SIZE as u64;
    let bytes_written: u64 = header_size + fat_total + root_dir_size + cluster_data_written;
    let total: u64 = volume_size as u64;
    if bytes_written > total { // wtf mode
        return Err(VFFError::Other(format!(
            "Data ({bytes_written:#x} bytes) exceeds the requested volume size ({total:#x} bytes)"
        )));
    }
    w.flush()?;
    // ftruncate to total
    w.into_inner().unwrap().set_len(total)?;
    Ok(())
}
