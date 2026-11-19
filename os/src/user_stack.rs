//! Linux 64 位进程入口栈的字节布局构造器。
//!
//! 本模块仅在调用方提供的内核字节缓冲区内完成数据编排，不分配内存、不建立页表映射，
//! 也不解引用任何用户虚拟地址。构造结果遵循 RISC-V 64 与 LoongArch 64 共用的 Linux
//! 进程启动 ABI（16 字节对齐的初始 `sp`），从低地址到高地址依次为：
//!
//! ```text
//! sp (16B 对齐) ──▶ argc (u64)
//!                   argv[0], ..., argv[argc - 1], 0
//!                   envp[0], ..., envp[n - 1], 0
//!                   auxv[0] (key, value), ..., (AT_NULL, 0)
//!                   零填充对齐区（0..15 字节）
//!                   argv / envp 字符串区（以 '\0' 结尾）
//! stack_top    ──▶ （缓冲区末端，半开上界）
//! ```

use thiserror::Error;

const WORD_SIZE: usize = 8;
const STACK_ALIGN: usize = 16;
const AT_NULL: usize = 0;

/// 辅助向量键：程序头表的用户虚拟地址。
pub const AT_PHDR: usize = 3;
/// 辅助向量键：单条程序头的字节数。
pub const AT_PHENT: usize = 4;
/// 辅助向量键：程序头条目总数。
pub const AT_PHNUM: usize = 5;
/// 辅助向量键：系统页面大小（字节）。
pub const AT_PAGESZ: usize = 6;
/// 辅助向量键：主程序入口的用户虚拟地址。
pub const AT_ENTRY: usize = 9;

/// 调用方提供的单条 ELF 辅助向量（`auxv`）记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuxEntry {
    /// Linux `AT_*` 类型编号，不得传入保留的终止键 `AT_NULL`（`0`）。
    pub key: usize,
    /// 对应的 64 位数值；若为指针类条目，必须使用目标用户虚拟地址。
    pub value: usize,
}

/// 构造初始用户栈失败的原因。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// 缓冲区长度大于 `stack_top`，无法表示对应的用户虚拟基址。
    #[error("initial stack virtual base address underflows")]
    AddressUnderflow,
    /// 计算字符串区、指针表或总布局字节数时发生算术溢出。
    #[error("initial stack layout size overflows")]
    SizeOverflow,
    /// 栈缓冲区不足以容纳全部数据及向下 16 字节对齐所需的填充。
    #[error("insufficient space for aligned initial stack")]
    InsufficientSpace,
    /// 参数或环境变量字节串内包含 `\0`，无法编码为单个 C 字符串。
    #[error("argument or environment string contains a NUL byte")]
    InteriorNul,
    /// `auxv` 中包含了保留的 `AT_NULL`（终止条目由 [`build`] 自动追加）。
    #[error("AT_NULL is reserved for the auxiliary vector terminator")]
    ReservedAuxKey,
}

/// 在给定缓冲区中构造 Linux 64 位初始用户栈，并返回 16 字节对齐的用户虚拟栈指针 `sp`。
///
/// 缓冲区 `stack` 对应用户虚拟地址半开区间 `[stack_top - stack.len(), stack_top)`，
/// `stack_top` 本身无需按 16 字节对齐。函数按原始输入顺序写入小端 64 位 `argc`、
/// 以 `0` 结尾的 `argv` 与 `envp` 指针数组、自动追加 `(AT_NULL, 0)` 终止项的 `auxv`、
/// 零填充对齐区以及以 `\0` 结尾的字符串区，其中所有字符串指针均已换算为目标用户虚拟地址。
///
/// 本函数仅负责写入上述入口结构；调用方需自行准备辅助向量中指针所引用的数据（如
/// `AT_PHDR`、`AT_RANDOM` 指向的内存），并在进入用户态前确保生成的字节位于对应的
/// 用户虚拟地址。成功时不会触碰 `sp` 以下的缓冲区字节；失败时保持 `stack` 原样不变。
///
/// # Errors
///
/// 当虚拟基址下溢、布局大小溢出、对齐后缓冲区容量不足，或输入包含内嵌 `\0` /
/// 保留键 `AT_NULL` 时，返回对应的 [`Error`](enum@Error)。
pub fn build(
    stack_top: usize,
    stack: &mut [u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
    auxv: &[AuxEntry],
) -> Result<usize, Error> {
    let base = stack_top
        .checked_sub(stack.len())
        .ok_or(Error::AddressUnderflow)?;
    let strings_len = argv.iter().chain(envp).try_fold(0usize, |len, arg| {
        if arg.contains(&0) {
            return Err(Error::InteriorNul);
        }
        len.checked_add(arg.len())
            .and_then(|len| len.checked_add(1))
            .ok_or(Error::SizeOverflow)
    })?;
    if auxv.iter().any(|entry| entry.key == AT_NULL) {
        return Err(Error::ReservedAuxKey);
    }
    // 除输入条目外，argc（1）、argv/envp 结尾空指针（2）及 auxv 终止对（2）共占 5 个机器字。
    let table_len = auxv
        .len()
        .checked_mul(2)
        .and_then(|words| words.checked_add(5))
        .and_then(|words| words.checked_add(argv.len()))
        .and_then(|words| words.checked_add(envp.len()))
        .and_then(|words| words.checked_mul(WORD_SIZE))
        .ok_or(Error::SizeOverflow)?;
    let payload_len = table_len
        .checked_add(strings_len)
        .ok_or(Error::SizeOverflow)?;
    if payload_len > stack.len() {
        return Err(Error::InsufficientSpace);
    }
    // 向下对齐至 16 字节边界可能额外消耗至多 15 字节，需相对 base 重新校验容量。
    let sp = (stack_top - payload_len) & !(STACK_ALIGN - 1);
    let start = sp.checked_sub(base).ok_or(Error::InsufficientSpace)?;

    // 所有长度及地址均已校验，至此开始才修改缓冲区，保证出错路径不产生部分写入。
    let (mut words, storage) = stack[start..].split_at_mut(table_len);
    let (padding, mut strings) = storage.split_at_mut(storage.len() - strings_len);
    padding.fill(0);

    let mut address = stack_top - strings_len;
    push_word(&mut words, argv.len());
    for list in [argv, envp] {
        for arg in list {
            let (dest, rest) = strings.split_at_mut(arg.len() + 1);
            dest[..arg.len()].copy_from_slice(arg);
            dest[arg.len()] = 0;
            strings = rest;
            push_word(&mut words, address);
            address += dest.len();
        }
        push_word(&mut words, 0);
    }
    for entry in auxv {
        push_word(&mut words, entry.key);
        push_word(&mut words, entry.value);
    }
    push_word(&mut words, AT_NULL);
    push_word(&mut words, 0);
    Ok(sp)
}

// 写入一个小端 64 位字并将可变切片游标前移；调用方已按 `table_len` 预留精确空间。
fn push_word(words: &mut &mut [u8], value: usize) {
    let (slot, rest) = core::mem::take(words).split_at_mut(WORD_SIZE);
    slot.copy_from_slice(&value.to_le_bytes());
    *words = rest;
}

#[cfg(feature = "ktest")]
#[path = "ktest/user_stack.rs"]
pub(crate) mod tests;
