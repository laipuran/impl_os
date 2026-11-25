//! 选择当前目标的用户分页实现。地址空间由共享内存管理层持有。

pub(crate) use super::riscv64::paging::{
    sv39_user_root::{Sv39UserRoot as UserRoot, USER_END},
    Permissions,
};
