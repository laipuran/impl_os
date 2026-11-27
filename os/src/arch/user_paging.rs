//! 选择当前目标的用户分页后端及物理页框地址转换方式。

pub(crate) use super::riscv64::paging::sv39_user_root::{
    frame_ptr, Sv39UserRoot as UserRoot, USER_END,
};

/// 检查当前是否未激活用户地址空间。
#[cfg(feature = "ktest")]
pub(crate) fn is_user_inactive() -> bool {
    super::riscv64::paging::is_bare()
}
