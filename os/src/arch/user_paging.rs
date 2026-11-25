//! 选择当前目标的用户分页与物理页初始化实现。地址空间由共享内存管理层持有。

pub(crate) use super::riscv64::paging::{
    sv39_user_root::{Sv39UserRoot as UserRoot, USER_END},
    user_frame::init_zeroed_frame,
};

/// 检查当前是否未激活用户地址空间。
#[cfg(feature = "ktest")]
pub(crate) fn is_user_inactive() -> bool {
    super::riscv64::paging::is_bare()
}
