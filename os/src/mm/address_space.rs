//! 独占用户页及其映射；构建期间地址空间保持非活动。

use alloc::vec::Vec;
use core::mem::MaybeUninit;

use crate::arch::{
    phys_access,
    user_paging::{Permissions, UserRoot, USER_END},
};

use super::{
    frame::{Frame, PAGE_SIZE},
    UserMappingError,
};

const STACK_PAGE: usize = USER_END - PAGE_SIZE;
const STACK_GUARD: usize = STACK_PAGE - PAGE_SIZE;

/// 用户映射的访问权限；所有组合都满足 W^X。
#[derive(Clone, Copy)]
pub(crate) enum Access {
    Read,
    ReadWrite,
    Execute,
    ReadExecute,
}

impl Access {
    fn permissions(self) -> Permissions {
        match self {
            Self::Read => Permissions::READ,
            Self::ReadWrite => Permissions::READ | Permissions::WRITE,
            Self::Execute => Permissions::EXECUTE,
            Self::ReadExecute => Permissions::READ | Permissions::EXECUTE,
        }
    }
}

/// 用户地址空间，持有页表及映射中引用的全部用户 RAM。
pub(crate) struct AddressSpace {
    // 先销毁页表，再归还叶页。
    table: UserRoot,
    pages: Vec<Frame<'static>>,
}

impl AddressSpace {
    /// 初始栈映射的半开上界。实际入口 SP 由启动栈布局决定。
    pub(crate) const STACK_TOP: usize = STACK_PAGE + PAGE_SIZE;

    /// 建立地址空间及一页用户栈，未提供内容的字节清零。
    ///
    /// # Errors
    ///
    /// 栈内容超过一页、后端拒绝构建或内存不足时返回错误。
    ///
    /// # Safety
    ///
    /// 当前须为关中断的启动主核，启动页框分配器已初始化且管理独占的真实 RAM。
    pub(crate) unsafe fn new(stack: &[u8]) -> Result<Self, UserMappingError> {
        if stack.len() > PAGE_SIZE {
            return Err(UserMappingError::InitialDataTooLarge);
        }
        // SAFETY: 调用方保证启动执行环境和页框分配器满足页表构建要求。
        let table = unsafe { UserRoot::new() }?;
        let mut space = Self {
            table,
            pages: Vec::new(),
        };
        space.insert_page(STACK_PAGE, Access::ReadWrite, 0, stack)?;
        Ok(space)
    }

    /// 检查整个页区间位于普通用户映射区，避开零页、栈和保护页。
    pub(crate) fn check_range(start: usize, end: usize) -> Result<(), UserMappingError> {
        if start < PAGE_SIZE
            || end > STACK_GUARD
            || start >= end
            || start % PAGE_SIZE != 0
            || end % PAGE_SIZE != 0
        {
            return Err(UserMappingError::InvalidUserAddress);
        }
        Ok(())
    }

    /// 映射独占用户页，将初始数据复制到指定页内偏移，其余字节清零。
    ///
    /// # Errors
    ///
    /// 地址、内容范围、页框分配或映射不符合要求时返回错误。
    pub(crate) fn map_page(
        &mut self,
        address: usize,
        access: Access,
        offset: usize,
        initial: &[u8],
    ) -> Result<(), UserMappingError> {
        let end = address
            .checked_add(PAGE_SIZE)
            .ok_or(UserMappingError::InvalidUserAddress)?;
        Self::check_range(address, end)?;
        self.insert_page(address, access, offset, initial)
    }

    /// 临时激活地址空间；闭包退出后恢复原地址空间。
    ///
    /// # Errors
    ///
    /// 当前寻址模式不支持激活时返回错误，闭包不执行。
    ///
    /// # Safety
    ///
    /// 当前须为关中断的启动主核。当前监管态栈（含保存在栈上的 Trap 现场）须位于
    /// 内核镜像的启动栈，闭包访问的内核对象须在切换前后保持同地址可访问。闭包须保持执行
    /// 环境前提，不访问未映射的表页，也不返回依赖活动映射的引用。
    pub(crate) unsafe fn with_active<R>(
        &mut self,
        run: impl FnOnce() -> R,
    ) -> Result<R, UserMappingError> {
        // SAFETY: 调用方保证启动栈、Trap 现场和闭包对象保持同地址可访问，
        // 并且不会软件访问未映射的表页；自有用户页在借用期间保持有效。
        unsafe { self.table.with_active(run) }
    }

    fn insert_page(
        &mut self,
        address: usize,
        access: Access,
        offset: usize,
        initial: &[u8],
    ) -> Result<(), UserMappingError> {
        let page = alloc_initialized_user_page(offset, initial)?;
        self.pages
            .try_reserve(1)
            .map_err(|_| UserMappingError::OutOfMemory)?;
        self.table.map_user(address, &page, access.permissions())?;
        self.pages.push(page);
        Ok(())
    }
}

/// 分配并完整初始化一页用户 RAM，未被初始数据覆盖的字节清零。
fn alloc_initialized_user_page(
    offset: usize,
    initial: &[u8],
) -> Result<Frame<'static>, UserMappingError> {
    let end = offset
        .checked_add(initial.len())
        .filter(|&end| end <= PAGE_SIZE)
        .ok_or(UserMappingError::InitialDataTooLarge)?;
    if !phys_access::is_bare() {
        return Err(UserMappingError::UnavailableInCurrentMode);
    }
    let mut page = super::frames()
        .alloc()
        .ok_or(UserMappingError::OutOfMemory)?;
    // SAFETY: 构建地址空间时使用启动分配器提供的独占 RAM；此页尚未映射，
    // 从检查 Bare 到初始化完成期间保持启动主核上的关中断执行。
    let bytes =
        unsafe { page.try_as_uninit_bytes_mut() }.ok_or(UserMappingError::InvalidMapping)?;
    bytes.fill(MaybeUninit::new(0));
    for (destination, &byte) in bytes[offset..end].iter_mut().zip(initial) {
        destination.write(byte);
    }
    Ok(page)
}

#[cfg(feature = "ktest")]
#[path = "../ktest/arch/riscv64/user_space/mod.rs"]
pub(crate) mod tests;
