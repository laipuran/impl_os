//! 当前寻址模式下的物理地址到内核地址转换。

use core::ptr::NonNull;

use super::paging::csr;

/// 当前是否使用 Bare 物理直通。
pub(crate) fn is_bare() -> bool {
    csr::satp() == 0
}

/// 将物理地址转换为内核指针，当前仅支持 Bare 模式下的非零地址。
///
/// 返回值只表达地址转换，不保证指向有效 RAM，也不建立借用。
pub(crate) fn kernel_ptr(physical: usize) -> Option<NonNull<u8>> {
    if !is_bare() {
        return None;
    }
    NonNull::new(physical as *mut u8)
}
