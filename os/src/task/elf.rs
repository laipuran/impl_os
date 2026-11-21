//! 面向用户程序装载的 ELF64 小端解析器。
//!
//! 本模块采用零堆分配、纯安全的字节切片解析方式校验不带程序解释器的 ELF64 `ET_EXEC`
//! 镜像，为后续地址空间装载与初始用户栈构造提供入口地址、程序头表元信息及各 `PT_LOAD`
//! 段的映射描述。
//!
//! 当前仅支持与目标架构匹配的小端 `ET_EXEC`（拒绝 `PT_INTERP` 与 `PN_XNUM` 扩展程序头
//! 计数）；不分配物理页、不建立页表映射，也不解引用任何用户虚拟地址。

use core::ops::Range;

use thiserror::Error;

use crate::{arch, mm::frame::PAGE_SIZE};

/// 校验 ELF64 镜像并提取程序入口、程序头表元信息与加载段视图。
///
/// 会完整检查文件头、程序头表边界、可选的 `PT_PHDR` 声明、所有 `PT_LOAD` 段的
/// 对齐/边界/升序/页级无重叠约束，以及入口点的架构指令对齐与可执行段归属。
/// 本函数不检查虚拟地址是否落入特定用户地址空间的合法范围，调用方在映射前需自行
/// 校验地址区间与权限策略。
///
/// # Errors
///
/// 当镜像截断、格式或架构不受支持、程序头表或 `PT_PHDR` 非法、加载段越界/溢出/乱序/
/// 页重叠，或程序入口非法时，返回对应的 [`Error`](enum@Error)。
pub fn parse(image: &[u8]) -> Result<Elf<'_>, Error> {
    let header = image.get(..HEADER_SIZE).ok_or(Error::TruncatedHeader)?;
    if &header[..4] != b"\x7fELF" {
        return Err(Error::BadMagic);
    }
    if header[4] != ELFCLASS64 {
        return Err(Error::UnsupportedClass);
    }
    if header[5] != ELFDATA2LSB {
        return Err(Error::UnsupportedEndian);
    }
    if header[6] != EV_CURRENT || u32_at(header, 20) != u32::from(EV_CURRENT) {
        return Err(Error::UnsupportedVersion);
    }
    if u16_at(header, 16) != ET_EXEC {
        return Err(Error::UnsupportedType);
    }
    if u16_at(header, 18) != arch::ELF_MACHINE {
        return Err(Error::WrongMachine);
    }
    if usize::from(u16_at(header, 52)) != HEADER_SIZE {
        return Err(Error::InvalidHeaderSize);
    }

    let entry = word_at(header, 24);
    if entry % arch::ELF_ENTRY_ALIGN != 0 {
        return Err(Error::InvalidEntry);
    }
    let file_offset = word_at(header, 32);
    let entry_size = usize::from(u16_at(header, 54));
    let count = u16_at(header, 56);
    // 0xffff 表示扩展计数，需要解析节头表；当前仅支持 ELF 头中直接给出的数量。
    if file_offset < HEADER_SIZE
        || file_offset % 8 != 0
        || entry_size != PHENT_SIZE
        || count == 0
        || count == PN_XNUM
    {
        return Err(Error::InvalidProgramHeaders);
    }
    let count = usize::from(count);
    let table_end = count
        .checked_mul(entry_size)
        .and_then(|size| file_offset.checked_add(size))
        .ok_or(Error::InvalidProgramHeaders)?;
    let table = image
        .get(file_offset..table_end)
        .ok_or(Error::InvalidProgramHeaders)?;

    let mut virtual_address = None;
    let mut declared_address = None;
    let mut seen_load = false;
    let mut entry_is_executable = false;
    let mut previous_start = 0;
    let mut previous_page_end = 0;
    for ph in table.chunks_exact(PHENT_SIZE) {
        match u32_at(ph, 0) {
            PT_LOAD => seen_load = true,
            PT_INTERP => return Err(Error::UnsupportedInterpreter),
            PT_PHDR => {
                // ELF 规范要求 PT_PHDR 若存在，必须唯一且位于所有 PT_LOAD 之前，
                // 并精确覆盖整张程序头表的文件内容（不含额外扩展或截断）。
                if seen_load
                    || declared_address.is_some()
                    || word_at(ph, 8) != file_offset
                    || word_at(ph, 32) != table.len()
                    || word_at(ph, 40) != table.len()
                {
                    return Err(Error::InvalidPhdrMapping);
                }
                declared_address = Some(word_at(ph, 16));
                continue;
            }
            _ => continue,
        }
        let load = LoadHeader::read(ph);
        let (file_end, memory_end) = load.validate(word_at(ph, 48), image.len())?;
        // 空加载段仍需通过基础合法性校验，但不占用任何虚拟页，因此跳过排序与重叠检查。
        if load.memory_size == 0 {
            continue;
        }
        let page_end = memory_end
            .checked_next_multiple_of(PAGE_SIZE)
            .ok_or(Error::SegmentAddressOverflow)?;
        if load.address < previous_start {
            return Err(Error::UnsortedLoadSegments);
        }
        if load.page_start() < previous_page_end {
            return Err(Error::OverlappingLoadPages);
        }
        previous_start = load.address;
        previous_page_end = page_end;

        if load.file_offset <= file_offset && table_end <= file_end {
            let address = load.address + (file_offset - load.file_offset);
            // 有 PT_PHDR 时只接受它所指向的副本，否则使用首个完整映射。
            if declared_address.map_or(virtual_address.is_none(), |declared| declared == address) {
                virtual_address = Some(address);
            }
        }
        if load.flags & PF_X != 0 && (load.address..memory_end).contains(&entry) {
            entry_is_executable = true;
        }
    }
    if declared_address.is_some() && virtual_address.is_none() {
        return Err(Error::InvalidPhdrMapping);
    }
    if !entry_is_executable {
        return Err(Error::InvalidEntry);
    }

    Ok(Elf {
        image,
        entry,
        headers: ProgramHeaders {
            file_offset,
            entry_size,
            count,
            virtual_address,
        },
        table,
    })
}

const HEADER_SIZE: usize = 64;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_EXEC: u16 = 2;
const PN_XNUM: u16 = 0xffff;
const PHENT_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PT_PHDR: u32 = 6;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

/// 已通过完整校验的 ELF64 镜像视图。
///
/// 借用原始镜像字节切片，按需构造装载描述，不持有额外堆内存。
#[derive(Debug)]
pub struct Elf<'a> {
    image: &'a [u8],
    entry: usize,
    headers: ProgramHeaders,
    table: &'a [u8],
}

impl<'a> Elf<'a> {
    /// 返回程序入口点（`e_entry`）的用户虚拟地址。
    pub fn entry(&self) -> usize {
        self.entry
    }

    /// 返回程序头表的文件位置、条目规格及映射后的用户虚拟地址。
    pub fn program_headers(&self) -> ProgramHeaders {
        self.headers
    }

    /// 按起始虚拟地址升序遍历所有非空 `PT_LOAD` 段的装载描述。
    ///
    /// 自动跳过 `p_memsz == 0` 的空加载段，迭代过程不触发堆分配。
    pub fn load_segments(&self) -> impl Iterator<Item = LoadSegment<'a>> + 'a {
        let image = self.image;
        self.table
            .chunks_exact(PHENT_SIZE)
            .filter(|ph| u32_at(ph, 0) == PT_LOAD)
            .map(LoadHeader::read)
            .filter(|load| load.memory_size != 0)
            .map(move |load| load.segment(image))
    }
}

struct LoadHeader {
    flags: u32,
    file_offset: usize,
    address: usize,
    file_size: usize,
    memory_size: usize,
}

impl LoadHeader {
    fn read(ph: &[u8]) -> Self {
        Self {
            flags: u32_at(ph, 4),
            file_offset: word_at(ph, 8),
            address: word_at(ph, 16),
            file_size: word_at(ph, 32),
            memory_size: word_at(ph, 40),
        }
    }

    fn page_start(&self) -> usize {
        self.address & !(PAGE_SIZE - 1)
    }

    fn validate(&self, align: usize, image_len: usize) -> Result<(usize, usize), Error> {
        if self.file_size > self.memory_size {
            return Err(Error::FileLargerThanMemory);
        }
        // PT_LOAD 始终须满足页内偏移同余，p_align 大于 1 时还须满足其额外约束。
        if self.file_offset % PAGE_SIZE != self.address % PAGE_SIZE
            || (align > 1
                && (!align.is_power_of_two() || self.file_offset % align != self.address % align))
        {
            return Err(Error::InvalidSegmentAlignment);
        }
        let file_end = if self.file_size == 0 {
            // 纯 BSS 不读文件，也不能为程序头表提供文件映射。
            0
        } else {
            self.file_offset
                .checked_add(self.file_size)
                .filter(|&end| end <= image_len)
                .ok_or(Error::SegmentFileOutOfBounds)?
        };
        let memory_end = self
            .address
            .checked_add(self.memory_size)
            .ok_or(Error::SegmentAddressOverflow)?;
        Ok((file_end, memory_end))
    }

    fn segment(self, image: &[u8]) -> LoadSegment<'_> {
        // parse 已校验所有区间和对齐上界，且镜像在借用期间不可变。
        let memory_end = self.address + self.memory_size;
        let page_start = self.page_start();
        LoadSegment {
            memory: self.address..memory_end,
            pages: page_start..memory_end.next_multiple_of(PAGE_SIZE),
            data: if self.file_size == 0 {
                &[]
            } else {
                &image[self.file_offset..self.file_offset + self.file_size]
            },
            data_offset: self.address - page_start,
            zero_fill: self.address + self.file_size..memory_end,
            permissions: Permissions {
                read: self.flags & PF_R != 0,
                write: self.flags & PF_W != 0,
                execute: self.flags & PF_X != 0,
            },
        }
    }
}

// 调用方仅以长度已校验的 ELF64 头（64 字节）或程序头（56 字节）及其固定偏移调用这些读取函数。
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(field(bytes, offset))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(field(bytes, offset))
}

fn word_at(bytes: &[u8], offset: usize) -> usize {
    usize::from_le_bytes(field(bytes, offset))
}

fn field<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    // 切片长度恒为 N，转换为定长数组不会失败。
    bytes[offset..offset + N].try_into().unwrap()
}

/// 解析或校验 ELF64 镜像失败的原因。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// 镜像长度不足 64 字节，无法容纳完整的 ELF64 文件头。
    #[error("truncated ELF64 header")]
    TruncatedHeader,
    /// 文件头前 4 字节不是 `\x7fELF` 魔数。
    #[error("invalid ELF magic")]
    BadMagic,
    /// 文件类别不是 64 位（`ELFCLASS64`）。
    #[error("unsupported ELF class")]
    UnsupportedClass,
    /// 字节序不是小端（`ELFDATA2LSB`）。
    #[error("unsupported ELF byte order")]
    UnsupportedEndian,
    /// 标识字节或文件头中的 ELF 版本不是 `EV_CURRENT`。
    #[error("unsupported ELF version")]
    UnsupportedVersion,
    /// 文件类型不是 `ET_EXEC`。
    #[error("unsupported ELF file type")]
    UnsupportedType,
    /// 目标机器号（`e_machine`）与当前编译架构不匹配。
    #[error("ELF machine does not match target architecture")]
    WrongMachine,
    /// 文件头声明的长度（`e_ehsize`）不等于 64 字节。
    #[error("invalid ELF64 header size")]
    InvalidHeaderSize,
    /// 程序头表的偏移、对齐、条目大小或数量非法，或未完整落在镜像内。
    #[error("invalid ELF program header table")]
    InvalidProgramHeaders,
    /// `PT_PHDR` 重复、出现在加载段之后、偏移/尺寸与表不符，或未被任何加载段文件数据完整覆盖。
    #[error("invalid ELF PT_PHDR mapping")]
    InvalidPhdrMapping,
    /// 镜像包含 `PT_INTERP`，当前不支持动态链接器/程序解释器。
    #[error("ELF program interpreters are unsupported")]
    UnsupportedInterpreter,
    /// 加载段的文件字节数（`p_filesz`）大于其内存字节数（`p_memsz`）。
    #[error("ELF load segment file size exceeds memory size")]
    FileLargerThanMemory,
    /// 加载段不满足页内偏移同余，或 `p_align > 1` 时非 2 的幂/不满足同余约束。
    #[error("invalid ELF load segment alignment")]
    InvalidSegmentAlignment,
    /// 加载段的文件数据区间发生溢出或超出镜像末尾。
    #[error("ELF load segment exceeds file bounds")]
    SegmentFileOutOfBounds,
    /// 加载段的虚拟地址或向上取整至页边界的区间发生算术溢出。
    #[error("ELF load segment virtual address overflows")]
    SegmentAddressOverflow,
    /// 非空加载段未按起始虚拟地址升序排列。
    #[error("ELF load segments are not ordered by virtual address")]
    UnsortedLoadSegments,
    /// 相邻的非空加载段占用了重叠的 4 KiB 虚拟页。
    #[error("ELF load segments occupy overlapping virtual pages")]
    OverlappingLoadPages,
    /// 程序入口未满足当前架构指令对齐要求，或未落在可执行加载段内。
    #[error("ELF entry point is misaligned or outside executable load segments")]
    InvalidEntry,
}

/// 程序头表的规格与映射位置。
///
/// 调用方可使用其中的信息填充 Linux 初始用户栈辅助向量中的
/// `AT_PHDR`、`AT_PHENT` 与 `AT_PHNUM` 条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramHeaders {
    /// 程序头表在原始镜像中的文件字节偏移（`e_phoff`）。
    pub file_offset: usize,
    /// 单条程序头的字节数（`e_phentsize`，固定为 56）。
    pub entry_size: usize,
    /// 程序头条目总数（`e_phnum`），包含不参与加载的条目。
    pub count: usize,
    /// 程序头表映射到用户态后的虚拟地址。
    ///
    /// 若镜像包含合法的 `PT_PHDR`，则采用其声明的地址；否则取首个文件数据完整覆盖
    /// 程序头表的非空 `PT_LOAD` 段所对应的地址。若无加载段覆盖完整表，则为 `None`。
    pub virtual_address: Option<usize>,
}

/// ELF 加载段请求的页面访问权限（对应 `p_flags` 中的 `PF_R`/`PF_W`/`PF_X`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    /// 允许读取（`PF_R`）。
    pub read: bool,
    /// 允许写入（`PF_W`）。
    pub write: bool,
    /// 允许执行（`PF_X`）。
    pub execute: bool,
}

/// 单个非空 `PT_LOAD` 段的装载描述。
///
/// 所有地址区间均表示用户虚拟地址的半开区间 `[start, end)`。
/// 装载方在按 [`Self::pages`] 建立页表映射时，除了拷贝 [`Self::data`] 并清零
/// [`Self::zero_fill`] 之外，还应确保首尾页内的对齐填充区域（`pages.start..memory.start`
/// 与 `memory.end..pages.end`）均已清零，避免泄露残留的物理页内容。
#[derive(Debug, PartialEq, Eq)]
pub struct LoadSegment<'a> {
    /// 段实际占用的虚拟内存字节区间（`[p_vaddr, p_vaddr + p_memsz)`），不含页对齐填充。
    pub memory: Range<usize>,
    /// 覆盖该段的完整 4 KiB 虚拟页区间，与其他加载段的 [`Self::pages`] 互不相交。
    pub pages: Range<usize>,
    /// 从原始镜像借用的待拷贝文件数据；纯 BSS 段为空切片。
    pub data: &'a [u8],
    /// 将 [`Self::data`] 写入 [`Self::pages`] 时相对 `pages.start` 的字节偏移。
    pub data_offset: usize,
    /// 紧随文件数据之后、必须清零的段内 BSS 虚拟地址区间；无 BSS 时为空区间。
    pub zero_fill: Range<usize>,
    /// 由 `PF_R`、`PF_W`、`PF_X` 解码得到的读、写、执行权限（不保留 `p_flags` 的其余标志位）；
    /// 是否允许该权限组合由目标地址空间策略决定。
    pub permissions: Permissions,
}

#[cfg(feature = "ktest")]
#[path = "../ktest/elf.rs"]
pub(crate) mod tests;
