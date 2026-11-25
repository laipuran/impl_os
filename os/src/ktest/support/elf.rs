//! 为解析和装载自检编码小端 ELF64 镜像。

use crate::mm::frame::PAGE_SIZE;

pub(crate) const ELF64_HEADER_SIZE: usize = 64;
pub(crate) const ELF64_PHENT_SIZE: usize = 56;
pub(crate) const PT_NULL: u32 = 0;
pub(crate) const PT_LOAD: u32 = 1;
pub(crate) const PT_INTERP: u32 = 3;
pub(crate) const PT_PHDR: u32 = 6;
pub(crate) const PF_X: u32 = 1;
pub(crate) const PF_W: u32 = 2;
pub(crate) const PF_R: u32 = 4;

pub(crate) const EH_MAGIC: usize = 0;
pub(crate) const EH_CLASS: usize = 4;
pub(crate) const EH_DATA: usize = 5;
pub(crate) const EH_IDENT_VERSION: usize = 6;
pub(crate) const EH_TYPE: usize = 16;
const EH_MACHINE: usize = 18;
pub(crate) const EH_VERSION: usize = 20;
pub(crate) const EH_ENTRY: usize = 24;
pub(crate) const EH_PHOFF: usize = 32;
pub(crate) const EH_EHSIZE: usize = 52;
pub(crate) const EH_PHENTSIZE: usize = 54;
pub(crate) const EH_PHNUM: usize = 56;

pub(crate) const PH_TYPE: usize = 0;
pub(crate) const PH_FLAGS: usize = 4;
pub(crate) const PH_OFFSET: usize = 8;
pub(crate) const PH_VADDR: usize = 16;
const PH_PADDR: usize = 24;
pub(crate) const PH_FILESZ: usize = 32;
pub(crate) const PH_MEMSZ: usize = 40;
pub(crate) const PH_ALIGN: usize = 48;

/// 将原始字节复制到 `image` 中从 `offset` 开始的区间，不做字节序转换。
///
/// # Panics
///
/// 如果目标区间超出 `image`，则触发 panic。
pub(crate) fn put(image: &mut [u8], offset: usize, bytes: &[u8]) {
    image[offset..offset + bytes.len()].copy_from_slice(bytes);
}

/// 从 `offset` 开始写入 2 字节的小端整数；目标区间超出 `image` 时触发 panic。
pub(crate) fn put_u16(image: &mut [u8], offset: usize, value: u16) {
    put(image, offset, &value.to_le_bytes());
}

/// 从 `offset` 开始写入 4 字节的小端整数；目标区间超出 `image` 时触发 panic。
pub(crate) fn put_u32(image: &mut [u8], offset: usize, value: u32) {
    put(image, offset, &value.to_le_bytes());
}

/// 从 `offset` 开始写入 8 字节的小端整数；目标区间超出 `image` 时触发 panic。
pub(crate) fn put_u64(image: &mut [u8], offset: usize, value: u64) {
    put(image, offset, &value.to_le_bytes());
}

/// 返回紧随 ELF 文件头的第 `index` 条程序头的文件偏移（从 0 计数）。
///
/// 仅计算偏移，不检查索引是否小于 `e_phnum`，也不检查镜像边界。
pub(crate) const fn ph_offset(index: usize) -> usize {
    ELF64_HEADER_SIZE + index * ELF64_PHENT_SIZE
}

/// 在镜像开头写入小端 ELF64 `ET_EXEC` 文件头，令 `e_phoff` 指向紧随文件头的程序头表。
///
/// 不写入程序头表；未编码的文件头字节由调用者预先清零。
///
/// # Panics
///
/// 如果 `image` 不足以容纳写入的字段（最后一个字段 `e_phnum` 结束于偏移 58），
/// 则触发 panic。完整的 ELF64 文件头占 64 字节。
pub(crate) fn put_header(image: &mut [u8], machine: u16, entry: u64, phnum: u16) {
    put(image, EH_MAGIC, b"\x7fELF");
    image[EH_CLASS] = 2; // ELFCLASS64
    image[EH_DATA] = 1; // ELFDATA2LSB
    image[EH_IDENT_VERSION] = 1; // EV_CURRENT
    put_u16(image, EH_TYPE, 2); // ET_EXEC
    put_u16(image, EH_MACHINE, machine);
    put_u32(image, EH_VERSION, 1); // EV_CURRENT
    put_u64(image, EH_ENTRY, entry);
    put_u64(image, EH_PHOFF, ph_offset(0) as u64);
    put_u16(image, EH_EHSIZE, ELF64_HEADER_SIZE as u16);
    put_u16(image, EH_PHENTSIZE, ELF64_PHENT_SIZE as u16);
    put_u16(image, EH_PHNUM, phnum);
}

/// 调用者指定的 `PT_LOAD` 字段。
pub(crate) struct LoadSpec {
    pub(crate) flags: u32,
    pub(crate) file_offset: u64,
    pub(crate) virtual_address: u64,
    pub(crate) file_size: u64,
    pub(crate) memory_size: u64,
}

/// 从 `header` 开始写入 56 字节的小端 `PT_LOAD` 程序头，不写入段数据。
///
/// 将 `p_paddr` 置零，`p_align` 设为页大小。不验证 `p_offset` 与 `p_vaddr`
/// 模 `p_align` 是否同余，因此也可用于构造反例。
///
/// # Panics
///
/// 如果该程序头的目标区间超出 `image`，则触发 panic。
pub(crate) fn put_load(image: &mut [u8], header: usize, spec: LoadSpec) {
    put_u32(image, header + PH_TYPE, PT_LOAD);
    put_u32(image, header + PH_FLAGS, spec.flags);
    put_u64(image, header + PH_OFFSET, spec.file_offset);
    put_u64(image, header + PH_VADDR, spec.virtual_address);
    put_u64(image, header + PH_PADDR, 0);
    put_u64(image, header + PH_FILESZ, spec.file_size);
    put_u64(image, header + PH_MEMSZ, spec.memory_size);
    put_u64(image, header + PH_ALIGN, PAGE_SIZE as u64);
}
