//! 在裸地址模式下清零并初始化独占用户页框。

use super::is_bare;
use crate::mm::{
    frame::{Frame, PAGE_SIZE},
    UserMappingError,
};

/// 清零物理页框 `frame`，并借出整页可变缓冲区原位执行 `init`。
///
/// # Errors
///
/// 当前地址转换不是 Bare 时返回 [`UserMappingError::UnavailableInCurrentMode`] 的转换结果；
/// 页框物理地址为零时返回 [`UserMappingError::InvalidMapping`] 的转换结果；
/// 或直接返回 `init` 产生的错误。
///
/// # Safety
///
/// - 仅可在关中断的启动主核 S-mode 调用。
/// - `frame` 必须对应真实、独占且可写的物理 RAM（不得来自合成记账分配器），在 Bare 下可通过同地址内核指针完整读写，且与任何存活的 Rust 引用不重叠。
/// - `init` 执行期间不得修改 `satp`，也不得开启中断或并发。
pub(crate) unsafe fn init_zeroed_frame<R, E: From<UserMappingError>>(
    frame: &mut Frame<'_>,
    init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> Result<R, E>,
) -> Result<R, E> {
    if !is_bare() {
        return Err(UserMappingError::UnavailableInCurrentMode.into());
    }
    let physical = frame.start_address();
    if physical == 0 {
        return Err(UserMappingError::InvalidMapping.into());
    }
    let buf = physical as *mut [u8; PAGE_SIZE];
    // SAFETY: 已确认处于 Bare 模式且物理地址非零；调用方保证页框独占、可同地址读写，且回调不改变地址转换或并发状态。
    // 先整页清零再建立可变引用，引用只在本次回调期间有效。
    unsafe {
        buf.write_bytes(0, 1);
        init(&mut *buf)
    }
}
