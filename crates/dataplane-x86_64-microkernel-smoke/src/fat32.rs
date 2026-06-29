use crate::layout::{BLOCK_SECTOR_BYTES, FS_SECTOR_OFFSET, FS_TASK_BYTES};

const BOOT_SIGNATURE_OFFSET: usize = 510;

pub(crate) const FAT32_BYTES_PER_SECTOR: u16 = 512;
pub(crate) const FAT32_EXPECTED_SECTORS_PER_CLUSTER: u8 = 1;
pub(crate) const FAT32_EXPECTED_RESERVED_SECTORS: u16 = 32;
pub(crate) const FAT32_EXPECTED_FAT_COUNT: u8 = 2;
pub(crate) const FAT32_EXPECTED_FAT_SECTORS: u32 = 1024;
pub(crate) const FAT32_EXPECTED_ROOT_CLUSTER: u32 = 2;
pub(crate) const FAT32_EOC_MIN: u32 = 0x0fff_fff8;
pub(crate) const FAT32_HELLO_CLUSTER: u32 = 3;
pub(crate) const FAT32_INDEX_CLUSTER: u32 = 4;
pub(crate) const FAT32_LARGE_CLUSTER: u32 = 6;
pub(crate) const FAT32_CHAIN_CLUSTER_FIRST: u32 = 7;
pub(crate) const FAT32_CHAIN_CLUSTER_SECOND: u32 = 8;
#[cfg(feature = "fat32-write-proof")]
pub(crate) const FAT32_OUT_CLUSTER: u32 = 5;
pub(crate) const FAT32_DIRECTORY_INDEX_ENTRY_CAP: usize = 4;
pub(crate) const FAT32_DIRECTORY_INDEX_NAME_CAP: usize = 12;
pub(crate) const FAT32_DIRECTORY_INDEX_RESPONSE_CAP: usize = 192;
pub(crate) const FAT32_HELLO_PATH: &str = "/HELLO.TXT";
pub(crate) const FAT32_INDEX_PATH: &str = "/INDEX.HTM";
pub(crate) const FAT32_LARGE_PATH: &str = "/LARGE.HTM";
pub(crate) const FAT32_CHAIN_PATH: &str = "/CHAIN.HTM";
pub(crate) const FAT32_HELLO_NAME: [u8; 11] = *b"HELLO   TXT";
pub(crate) const FAT32_INDEX_NAME: [u8; 11] = *b"INDEX   HTM";
pub(crate) const FAT32_LARGE_NAME: [u8; 11] = *b"LARGE   HTM";
pub(crate) const FAT32_CHAIN_NAME: [u8; 11] = *b"CHAIN   HTM";
pub(crate) const FAT32_LARGE_CONTENT_BYTES: u32 = 480;
pub(crate) const FAT32_CHAIN_CONTENT_BYTES: u32 = 834;
#[cfg(feature = "fat32-write-proof")]
pub(crate) const FAT32_OUT_NAME: [u8; 11] = *b"OUT     TXT";
pub(crate) const FAT32_HELLO_CONTENT: &[u8] = b"hello from dataplane microkernel fat32\r\n";
pub(crate) const FAT32_INDEX_CONTENT: &[u8] = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n";
#[cfg(feature = "fat32-write-proof")]
pub(crate) const FAT32_OUT_CONTENT: &[u8] = b"dataplane guest fat32 write proof\r\n";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FsKnownPath {
    Hello,
    Index,
    Large,
    Chain,
}

impl FsKnownPath {
    pub(crate) fn from_path_bytes(path: &[u8]) -> Option<Self> {
        if path == FAT32_HELLO_PATH.as_bytes() {
            Some(Self::Hello)
        } else if path == FAT32_INDEX_PATH.as_bytes() {
            Some(Self::Index)
        } else if path == FAT32_LARGE_PATH.as_bytes() {
            Some(Self::Large)
        } else if path == FAT32_CHAIN_PATH.as_bytes() {
            Some(Self::Chain)
        } else {
            None
        }
    }

    pub(crate) fn is_cli_readable(self) -> bool {
        matches!(self, Self::Hello | Self::Index)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Hello => FAT32_HELLO_PATH,
            Self::Index => FAT32_INDEX_PATH,
            Self::Large => FAT32_LARGE_PATH,
            Self::Chain => FAT32_CHAIN_PATH,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FsOperation {
    Open,
    Stat,
    ReadAt,
    ListRoot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FsError {
    Layout,
    Root,
    Block,
    Handle,
    Capacity,
    Range,
}

impl FsError {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Layout => "fs-layout",
            Self::Root => "fs-root",
            Self::Block => "fs-block",
            Self::Handle => "fs-handle",
            Self::Capacity => "fs-capacity",
            Self::Range => "fs-range",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsFileHandle {
    pub(crate) path: FsKnownPath,
    pub(crate) cluster: u32,
    pub(crate) size: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct FsReadResult {
    pub(crate) handle: FsFileHandle,
    pub(crate) offset: u32,
    pub(crate) size: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct FsRootListing {
    pub(crate) hello: FsFileStat,
    pub(crate) index: FsFileStat,
    pub(crate) large: FsFileStat,
    pub(crate) chain: FsFileStat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsDirectoryIndexRow {
    pub(crate) path: &'static str,
    pub(crate) cluster: u32,
    pub(crate) size: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsDirectoryIndexProof {
    pub(crate) hello: FsDirectoryIndexRow,
    pub(crate) index: FsDirectoryIndexRow,
    pub(crate) large: FsDirectoryIndexRow,
    pub(crate) chain: FsDirectoryIndexRow,
    pub(crate) entry_count: u8,
    pub(crate) entry_cap: u8,
    pub(crate) name_cap: u8,
    pub(crate) response_cap: u16,
    pub(crate) response_bytes: u16,
}

#[derive(Clone, Copy)]
pub(crate) enum FsServiceRequest {
    Open {
        path: FsKnownPath,
    },
    Stat {
        handle: FsFileHandle,
    },
    ReadAt {
        handle: FsFileHandle,
        offset: u32,
        len: u32,
    },
    ListRoot,
}

impl FsServiceRequest {
    pub(crate) fn operation(self) -> FsOperation {
        match self {
            Self::Open { .. } => FsOperation::Open,
            Self::Stat { .. } => FsOperation::Stat,
            Self::ReadAt { .. } => FsOperation::ReadAt,
            Self::ListRoot => FsOperation::ListRoot,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FsServiceReply {
    Open(FsFileHandle),
    Stat(FsFileStat),
    Read(FsReadResult),
    ListRoot(FsRootListing),
}

#[derive(Clone, Copy)]
pub(crate) enum FsNegativeCase {
    UnsupportedPath,
    LongFilename,
    UnsupportedWrite,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct FsFileStat {
    pub(crate) path: &'static str,
    pub(crate) cluster: u32,
    pub(crate) size: u32,
}

pub(crate) struct FsRejection {
    pub(crate) path: &'static str,
    pub(crate) reason: &'static str,
    pub(crate) marker: &'static str,
}

pub(crate) struct FsTask {
    max_root_entries: u8,
}

impl FsTask {
    pub(crate) const fn new() -> Self {
        Self {
            max_root_entries: 16,
        }
    }

    pub(crate) fn parse_boot_sector(
        &self,
        memory: &FsTaskMemory<'_>,
    ) -> Result<Fat32Layout, &'static str> {
        let sector = memory.sector();
        if sector[BOOT_SIGNATURE_OFFSET] != 0x55 || sector[BOOT_SIGNATURE_OFFSET + 1] != 0xaa {
            return Err("fat32-boot-signature");
        }
        if read_le_u16(sector, 11) != FAT32_BYTES_PER_SECTOR {
            return Err("fat32-bytes-per-sector");
        }
        if sector[13] != FAT32_EXPECTED_SECTORS_PER_CLUSTER {
            return Err("fat32-sectors-per-cluster");
        }
        if read_le_u16(sector, 14) != FAT32_EXPECTED_RESERVED_SECTORS {
            return Err("fat32-reserved-sectors");
        }
        if sector[16] != FAT32_EXPECTED_FAT_COUNT {
            return Err("fat32-fat-count");
        }
        if read_le_u32(sector, 36) != FAT32_EXPECTED_FAT_SECTORS {
            return Err("fat32-fat-sectors");
        }
        if read_le_u32(sector, 44) != FAT32_EXPECTED_ROOT_CLUSTER {
            return Err("fat32-root-cluster");
        }
        if &sector[82..90] != b"FAT32   " {
            return Err("fat32-label");
        }

        Ok(Fat32Layout {
            reserved_sectors: read_le_u16(sector, 14) as u32,
            fat_count: sector[16] as u32,
            fat_sectors: read_le_u32(sector, 36),
            sectors_per_cluster: sector[13] as u32,
            root_cluster: read_le_u32(sector, 44),
        })
    }

    pub(crate) fn verify_fat_entries(&self, memory: &FsTaskMemory<'_>) -> Result<(), &'static str> {
        let sector = memory.sector();
        for cluster in [
            FAT32_EXPECTED_ROOT_CLUSTER,
            FAT32_HELLO_CLUSTER,
            FAT32_INDEX_CLUSTER,
            FAT32_LARGE_CLUSTER,
        ] {
            let offset = cluster as usize * 4;
            if read_le_u32(sector, offset) < FAT32_EOC_MIN {
                return Err("fat32-fat-entry");
            }
        }
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn write_fat_eoc(
        &self,
        memory: &mut FsTaskMemory<'_>,
        cluster: u32,
    ) -> Result<(), &'static str> {
        if cluster != FAT32_OUT_CLUSTER {
            return Err("fat32-write-cluster");
        }
        let offset = fat_entry_sector_offset(cluster);
        let sector = memory.sector_mut();
        write_le_u32(&mut sector[offset..offset + 4], FAT32_EOC_MIN);
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn verify_fat_eoc(
        &self,
        memory: &FsTaskMemory<'_>,
        cluster: u32,
    ) -> Result<(), &'static str> {
        let offset = fat_entry_sector_offset(cluster);
        if read_le_u32(memory.sector(), offset) < FAT32_EOC_MIN {
            return Err("fat32-write-fat-readback");
        }
        Ok(())
    }

    pub(crate) fn parse_root_directory(
        &self,
        memory: &FsTaskMemory<'_>,
    ) -> Result<RootDirectoryProof, &'static str> {
        if self.max_root_entries < 4 {
            return Err("fat32-root-capacity");
        }
        if !path_is_absolute(FAT32_HELLO_PATH)
            || !path_is_absolute(FAT32_INDEX_PATH)
            || !path_is_absolute(FAT32_LARGE_PATH)
            || !path_is_absolute(FAT32_CHAIN_PATH)
        {
            return Err("fat32-path-proof");
        }
        let sector = memory.sector();
        let hello = parse_short_entry(&sector[0..32], FAT32_HELLO_NAME)?;
        let index = parse_short_entry(&sector[32..64], FAT32_INDEX_NAME)?;
        let large = parse_short_entry(&sector[64..96], FAT32_LARGE_NAME)?;
        let chain = parse_short_entry(&sector[96..128], FAT32_CHAIN_NAME)?;
        if hello.cluster != FAT32_HELLO_CLUSTER || hello.size != FAT32_HELLO_CONTENT.len() as u32 {
            return Err("fat32-hello-entry");
        }
        if index.cluster != FAT32_INDEX_CLUSTER || index.size != FAT32_INDEX_CONTENT.len() as u32 {
            return Err("fat32-index-entry");
        }
        if large.cluster != FAT32_LARGE_CLUSTER || large.size != FAT32_LARGE_CONTENT_BYTES {
            return Err("fat32-large-entry");
        }
        if chain.cluster != FAT32_CHAIN_CLUSTER_FIRST || chain.size != FAT32_CHAIN_CONTENT_BYTES {
            return Err("fat32-chain-entry");
        }
        Ok(RootDirectoryProof {
            hello_cluster: hello.cluster,
            hello_size: hello.size,
            index_cluster: index.cluster,
            index_size: index.size,
            large_cluster: large.cluster,
            large_size: large.size,
            chain_cluster: chain.cluster,
            chain_size: chain.size,
        })
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn write_out_directory_entry(
        &self,
        memory: &mut FsTaskMemory<'_>,
    ) -> Result<(), &'static str> {
        if self.max_root_entries < 4 {
            return Err("fat32-out-root-capacity");
        }
        let mut slot_offset = None;
        {
            let sector = memory.sector();
            let root_entry_limit = self.max_root_entries as usize;
            for entry_index in 3..root_entry_limit {
                let offset = entry_index * 32;
                let entry = &sector[offset..offset + 32];
                if entry[0] == 0x00 || entry[0..11] == FAT32_OUT_NAME[..] {
                    slot_offset = Some(offset);
                    break;
                }
            }
        }
        let slot_offset = slot_offset.ok_or("fat32-out-root-slot")?;
        let entry = &mut memory.sector_mut()[slot_offset..slot_offset + 32];
        if entry[0] != 0x00 && entry[0..11] != FAT32_OUT_NAME[..] {
            return Err("fat32-out-root-slot");
        }
        for byte in entry.iter_mut() {
            *byte = 0;
        }
        entry[0..11].copy_from_slice(&FAT32_OUT_NAME);
        entry[11] = 0x20;
        write_le_u16(&mut entry[20..22], (FAT32_OUT_CLUSTER >> 16) as u16);
        write_le_u16(&mut entry[26..28], FAT32_OUT_CLUSTER as u16);
        write_le_u32(&mut entry[28..32], FAT32_OUT_CONTENT.len() as u32);
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn parse_out_directory_entry(
        &self,
        memory: &FsTaskMemory<'_>,
    ) -> Result<ShortEntry, &'static str> {
        if self.max_root_entries < 4 {
            return Err("fat32-out-root-capacity");
        }
        let sector = memory.sector();
        for entry_index in 3..self.max_root_entries as usize {
            let offset = entry_index * 32;
            let entry = &sector[offset..offset + 32];
            if entry[0] == 0x00 {
                break;
            }
            if entry[0] == 0xE5 || entry[11] == 0x0F {
                continue;
            }
            if entry[0..11] == FAT32_OUT_NAME[..] {
                return parse_short_entry(entry, FAT32_OUT_NAME);
            }
        }
        Err("fat32-out-root-slot")
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn write_file_content(
        &self,
        memory: &mut FsTaskMemory<'_>,
        content: &[u8],
    ) -> Result<(), &'static str> {
        if content.len() > BLOCK_SECTOR_BYTES {
            return Err("fat32-write-content-capacity");
        }
        let sector = memory.sector_mut();
        for byte in sector.iter_mut() {
            *byte = 0;
        }
        sector[..content.len()].copy_from_slice(content);
        Ok(())
    }

    pub(crate) fn verify_file_content(
        &self,
        memory: &FsTaskMemory<'_>,
        expected: &[u8],
    ) -> Result<(), &'static str> {
        let sector = memory.sector();
        if &sector[..expected.len()] != expected {
            return Err("fat32-file-content");
        }
        Ok(())
    }

    pub(crate) fn open(
        &self,
        root: RootDirectoryProof,
        path: FsKnownPath,
    ) -> Result<FsFileHandle, FsError> {
        let stat = self.stat_from_root(root, path)?;
        Ok(FsFileHandle {
            path,
            cluster: stat.cluster,
            size: stat.size,
        })
    }

    pub(crate) fn stat(&self, handle: FsFileHandle) -> Result<FsFileStat, FsError> {
        self.validate_handle(handle)?;
        Ok(FsFileStat {
            path: handle.path.as_str(),
            cluster: handle.cluster,
            size: handle.size,
        })
    }

    pub(crate) fn list_root(&self, root: RootDirectoryProof) -> Result<FsRootListing, FsError> {
        if self.max_root_entries < FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8 {
            return Err(FsError::Capacity);
        }
        Ok(FsRootListing {
            hello: self.stat_from_root(root, FsKnownPath::Hello)?,
            index: self.stat_from_root(root, FsKnownPath::Index)?,
            large: self.stat_from_root(root, FsKnownPath::Large)?,
            chain: self.stat_from_root(root, FsKnownPath::Chain)?,
        })
    }

    pub(crate) fn directory_index_view(
        &self,
        root: FsRootListing,
    ) -> Result<FsDirectoryIndexProof, FsError> {
        let proof = FsDirectoryIndexProof {
            hello: FsDirectoryIndexRow {
                path: root.hello.path,
                cluster: root.hello.cluster,
                size: root.hello.size,
            },
            index: FsDirectoryIndexRow {
                path: root.index.path,
                cluster: root.index.cluster,
                size: root.index.size,
            },
            large: FsDirectoryIndexRow {
                path: root.large.path,
                cluster: root.large.cluster,
                size: root.large.size,
            },
            chain: FsDirectoryIndexRow {
                path: root.chain.path,
                cluster: root.chain.cluster,
                size: root.chain.size,
            },
            entry_count: FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8,
            entry_cap: FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8,
            name_cap: FAT32_DIRECTORY_INDEX_NAME_CAP as u8,
            response_cap: FAT32_DIRECTORY_INDEX_RESPONSE_CAP as u16,
            response_bytes: 188,
        };
        if proof.hello.path != FAT32_HELLO_PATH
            || proof.index.path != FAT32_INDEX_PATH
            || proof.large.path != FAT32_LARGE_PATH
            || proof.chain.path != FAT32_CHAIN_PATH
        {
            return Err(FsError::Root);
        }
        if proof.hello.path.len() > FAT32_DIRECTORY_INDEX_NAME_CAP
            || proof.index.path.len() > FAT32_DIRECTORY_INDEX_NAME_CAP
            || proof.large.path.len() > FAT32_DIRECTORY_INDEX_NAME_CAP
            || proof.chain.path.len() > FAT32_DIRECTORY_INDEX_NAME_CAP
        {
            return Err(FsError::Capacity);
        }
        if proof.response_bytes > proof.response_cap {
            return Err(FsError::Capacity);
        }
        Ok(proof)
    }

    pub(crate) fn read_at(
        &self,
        layout: Fat32Layout,
        mut read_sector: impl FnMut(u32) -> Result<(), FsError>,
        memory: &FsTaskMemory<'_>,
        handle: FsFileHandle,
        offset: u32,
        len: u32,
        out: &mut [u8],
    ) -> Result<FsReadResult, FsError> {
        self.validate_handle(handle)?;
        if offset >= handle.size || len > handle.size - offset {
            return Err(FsError::Range);
        }
        let len = len as usize;
        let offset = offset as usize;
        if len > out.len() {
            return Err(FsError::Capacity);
        }
        let mut remaining = len;
        let mut out_index = 0usize;
        let mut current_cluster = handle.cluster;
        let mut current_offset = offset;
        let mut skipped_clusters = current_offset / BLOCK_SECTOR_BYTES;
        while skipped_clusters > 0 {
            let fat_sector = layout.fat_start_sector()
                + (current_cluster * 4 / u32::from(FAT32_BYTES_PER_SECTOR));
            read_sector(fat_sector).map_err(|_| FsError::Block)?;
            let entry_offset = (current_cluster as usize * 4) % usize::from(FAT32_BYTES_PER_SECTOR);
            let next_cluster = read_le_u32(memory.sector(), entry_offset);
            if next_cluster >= FAT32_EOC_MIN {
                return Err(FsError::Range);
            }
            current_cluster = next_cluster;
            skipped_clusters -= 1;
        }
        current_offset %= BLOCK_SECTOR_BYTES;
        while remaining > 0 {
            let sector_lba = layout.cluster_sector(current_cluster);
            read_sector(sector_lba).map_err(|_| FsError::Block)?;
            let sector = memory.sector();
            let mut bytes_this_cluster = BLOCK_SECTOR_BYTES - current_offset;
            if bytes_this_cluster > remaining {
                bytes_this_cluster = remaining;
            }
            let mut index = 0usize;
            while index < bytes_this_cluster {
                out[out_index + index] = sector[current_offset + index];
                index += 1;
            }
            remaining -= bytes_this_cluster;
            out_index += bytes_this_cluster;
            current_offset = 0;
            if remaining > 0 {
                if handle.path == FsKnownPath::Chain {
                    if current_cluster != FAT32_CHAIN_CLUSTER_FIRST {
                        return Err(FsError::Range);
                    }
                    current_cluster = FAT32_CHAIN_CLUSTER_SECOND;
                } else {
                    return Err(FsError::Range);
                }
            }
        }
        Ok(FsReadResult {
            handle,
            offset: offset as u32,
            size: len,
        })
    }

    fn stat_from_root(
        &self,
        root: RootDirectoryProof,
        path: FsKnownPath,
    ) -> Result<FsFileStat, FsError> {
        match path {
            FsKnownPath::Hello => {
                if root.hello_cluster != FAT32_HELLO_CLUSTER
                    || root.hello_size != FAT32_HELLO_CONTENT.len() as u32
                {
                    return Err(FsError::Root);
                }
                Ok(FsFileStat {
                    path: FAT32_HELLO_PATH,
                    cluster: root.hello_cluster,
                    size: root.hello_size,
                })
            }
            FsKnownPath::Index => {
                if root.index_cluster != FAT32_INDEX_CLUSTER
                    || root.index_size != FAT32_INDEX_CONTENT.len() as u32
                {
                    return Err(FsError::Root);
                }
                Ok(FsFileStat {
                    path: FAT32_INDEX_PATH,
                    cluster: root.index_cluster,
                    size: root.index_size,
                })
            }
            FsKnownPath::Large => {
                if root.large_cluster != FAT32_LARGE_CLUSTER
                    || root.large_size != FAT32_LARGE_CONTENT_BYTES
                {
                    return Err(FsError::Root);
                }
                Ok(FsFileStat {
                    path: FAT32_LARGE_PATH,
                    cluster: root.large_cluster,
                    size: root.large_size,
                })
            }
            FsKnownPath::Chain => {
                if root.chain_cluster != FAT32_CHAIN_CLUSTER_FIRST
                    || root.chain_size != FAT32_CHAIN_CONTENT_BYTES
                {
                    return Err(FsError::Root);
                }
                Ok(FsFileStat {
                    path: FAT32_CHAIN_PATH,
                    cluster: root.chain_cluster,
                    size: root.chain_size,
                })
            }
        }
    }

    pub(crate) fn validate_handle(&self, handle: FsFileHandle) -> Result<(), FsError> {
        let valid = match handle.path {
            FsKnownPath::Hello => {
                handle.cluster == FAT32_HELLO_CLUSTER
                    && handle.size == FAT32_HELLO_CONTENT.len() as u32
            }
            FsKnownPath::Index => {
                handle.cluster == FAT32_INDEX_CLUSTER
                    && handle.size == FAT32_INDEX_CONTENT.len() as u32
            }
            FsKnownPath::Large => {
                handle.cluster == FAT32_LARGE_CLUSTER && handle.size == FAT32_LARGE_CONTENT_BYTES
            }
            FsKnownPath::Chain => {
                handle.cluster == FAT32_CHAIN_CLUSTER_FIRST
                    && handle.size == FAT32_CHAIN_CONTENT_BYTES
            }
        };
        if valid {
            Ok(())
        } else {
            Err(FsError::Handle)
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Fat32Layout {
    reserved_sectors: u32,
    fat_count: u32,
    pub(crate) fat_sectors: u32,
    sectors_per_cluster: u32,
    root_cluster: u32,
}

impl Fat32Layout {
    pub(crate) fn fat_start_sector(self) -> u32 {
        self.reserved_sectors
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn fat_entry_sector(self, cluster: u32) -> u32 {
        self.fat_start_sector() + cluster * 4 / u32::from(FAT32_BYTES_PER_SECTOR)
    }

    fn data_start_sector(self) -> u32 {
        self.reserved_sectors + self.fat_count * self.fat_sectors
    }

    pub(crate) fn cluster_sector(self, cluster: u32) -> u32 {
        self.data_start_sector() + (cluster - 2) * self.sectors_per_cluster
    }

    pub(crate) fn root_sector(self) -> u32 {
        self.cluster_sector(self.root_cluster)
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn sectors_per_cluster(self) -> u32 {
        self.sectors_per_cluster
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ShortEntry {
    pub(crate) cluster: u32,
    pub(crate) size: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct RootDirectoryProof {
    pub(crate) hello_cluster: u32,
    pub(crate) hello_size: u32,
    pub(crate) index_cluster: u32,
    pub(crate) index_size: u32,
    pub(crate) large_cluster: u32,
    pub(crate) large_size: u32,
    pub(crate) chain_cluster: u32,
    pub(crate) chain_size: u32,
}

pub(crate) struct FsTaskMemory<'a> {
    pub(crate) bytes: &'a mut [u8; FS_TASK_BYTES],
}

impl FsTaskMemory<'_> {
    pub(crate) fn sector(&self) -> &[u8] {
        &self.bytes[FS_SECTOR_OFFSET..FS_SECTOR_OFFSET + BLOCK_SECTOR_BYTES]
    }

    #[cfg(feature = "fat32-write-proof")]
    fn sector_mut(&mut self) -> &mut [u8] {
        &mut self.bytes[FS_SECTOR_OFFSET..FS_SECTOR_OFFSET + BLOCK_SECTOR_BYTES]
    }
}

fn parse_short_entry(entry: &[u8], expected_name: [u8; 11]) -> Result<ShortEntry, &'static str> {
    if entry.len() != 32 {
        return Err("fat32-entry-len");
    }
    if entry[0..11] != expected_name[..] {
        return Err("fat32-entry-name");
    }
    if entry[11] != 0x20 {
        return Err("fat32-entry-attr");
    }
    let cluster_high = u32::from(read_le_u16(entry, 20));
    let cluster_low = u32::from(read_le_u16(entry, 26));
    Ok(ShortEntry {
        cluster: (cluster_high << 16) | cluster_low,
        size: read_le_u32(entry, 28),
    })
}

fn path_is_absolute(path: &str) -> bool {
    matches!(path.as_bytes().first(), Some(b'/'))
}

#[cfg(feature = "fat32-write-proof")]
fn fat_entry_sector_offset(cluster: u32) -> usize {
    (cluster as usize * 4) % usize::from(FAT32_BYTES_PER_SECTOR)
}

fn read_le_u16(region: &[u8], offset: usize) -> u16 {
    u16::from(region[offset]) | (u16::from(region[offset + 1]) << 8)
}

#[cfg(feature = "fat32-write-proof")]
fn write_le_u16(dst: &mut [u8], value: u16) {
    dst[0] = value as u8;
    dst[1] = (value >> 8) as u8;
}

fn read_le_u32(region: &[u8], offset: usize) -> u32 {
    u32::from(region[offset])
        | (u32::from(region[offset + 1]) << 8)
        | (u32::from(region[offset + 2]) << 16)
        | (u32::from(region[offset + 3]) << 24)
}

#[cfg(feature = "fat32-write-proof")]
fn write_le_u32(dst: &mut [u8], value: u32) {
    dst[0] = value as u8;
    dst[1] = (value >> 8) as u8;
    dst[2] = (value >> 16) as u8;
    dst[3] = (value >> 24) as u8;
}
