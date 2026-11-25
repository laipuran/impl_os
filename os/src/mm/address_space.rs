//! 独占用户页及其映射；构建期间地址空间保持非活动。
//!
//! [`AddressSpace`] 声明字段时确保先销毁底层页表、再归还映射所引用的用户叶页，避免析构期间产生指向已释放物理页的悬垂表项。

use alloc::vec::Vec;

use crate::arch::user_paging::{self, UserRoot, USER_END};

use super::{
    frame::{Frame, FrameAllocator, PAGE_SIZE},
    Access, UserMappingError,
};

const STACK_PAGE: usize = USER_END - PAGE_SIZE;
const STACK_GUARD: usize = STACK_PAGE - PAGE_SIZE;

/// 用户地址空间，持有页表及映射中引用的全部用户 RAM。
pub(crate) struct AddressSpace<'a> {
    // 先销毁页表，再归还叶页。
    table: UserRoot<'a>,
    pages: Vec<Frame<'a>>,
    frames: &'a FrameAllocator,
}

#[cfg(feature = "ktest")]
impl AddressSpace<'static> {
    /// 使用全局分配器建立地址空间及一页清零的用户栈。
    ///
    /// # Errors
    ///
    /// 当前寻址模式不支持构建时返回 [`UserMappingError::UnavailableInCurrentMode`]；
    /// 页框或堆空间不足时返回 [`UserMappingError::OutOfMemory`]；
    /// 底层映射校验失败时返回 [`UserMappingError::InvalidMapping`]。
    /// 失败时回收本次已申请的全部页框。
    ///
    /// # Panics
    ///
    /// 内存管理尚未初始化时 panic（见 [`crate::mm::frames`]）。
    ///
    /// # Safety
    ///
    /// - 仅在关中断的启动主核特权态调用和使用，并满足 [`UserRoot::new`] 与 [`user_paging::init_zeroed_frame`] 的当前架构前提。
    /// - 全局页框分配器必须已初始化，并在借用期间提供可由当前架构内核指针独占读写的真实 RAM 页框。
    pub(crate) unsafe fn new() -> Result<Self, UserMappingError> {
        // SAFETY: 调用方保证启动执行环境和全局分配器满足构建要求。
        unsafe { Self::new_in(crate::mm::frames()) }
    }
}

impl<'a> AddressSpace<'a> {
    /// 初始用户栈映射的用户虚拟地址半开上界（不包含该地址）。
    pub(crate) const STACK_TOP: usize = STACK_PAGE + PAGE_SIZE;

    /// 使用指定分配器建立带清零用户栈的地址空间。
    ///
    /// # Errors
    ///
    /// 当底层分页后端拒绝构建或资源不足时返回对应的 [`UserMappingError`]，失败时回收本次已申请的所有页框。
    ///
    /// # Safety
    ///
    /// 调用方须满足 [`Self::new_with_stack_in`] 的全部安全前提。
    #[cfg(feature = "ktest")]
    unsafe fn new_in(frames: &'a FrameAllocator) -> Result<Self, UserMappingError> {
        // SAFETY: 调用方保证分配器和启动执行环境满足构建要求；空回调不改变执行环境。
        let (space, ()) =
            unsafe { Self::new_with_stack_in(frames, |_| Ok::<_, UserMappingError>(())) }?;
        Ok(space)
    }

    /// 使用指定分配器建立地址空间，并在已清零的物理栈页上原位构造初始用户栈。
    ///
    /// 传入 `init` 的页内缓冲区对应用户虚拟区间 `[STACK_TOP - PAGE_SIZE, STACK_TOP)`。
    ///
    /// # Errors
    ///
    /// 当前寻址模式不支持构建时返回 [`UserMappingError::UnavailableInCurrentMode`] 的转换结果；
    /// 页框或堆空间不足时返回 [`UserMappingError::OutOfMemory`] 的转换结果；
    /// 底层映射校验失败时返回 [`UserMappingError::InvalidMapping`] 的转换结果；
    /// 或直接返回 `init` 产生的错误。失败时回收本次构建已申请的全部页框。
    ///
    /// # Safety
    ///
    /// - 仅在关中断的启动主核特权态调用和使用，并满足 [`UserRoot::new`] 与 [`user_paging::init_zeroed_frame`] 的当前架构前提。
    /// - `frames` 在借用期间须提供可由当前架构内核指针独占访问的真实 RAM 页框。
    /// - `init` 执行期间不得修改地址转换配置，也不得开启中断或并发。
    pub(crate) unsafe fn new_with_stack_in<R, E: From<UserMappingError>>(
        frames: &'a FrameAllocator,
        init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> Result<R, E>,
    ) -> Result<(Self, R), E> {
        // SAFETY: 调用方保证分配器和启动执行环境满足构建要求。
        let table = unsafe { UserRoot::new(frames) }?;
        let mut space = Self {
            table,
            pages: Vec::new(),
            frames,
        };
        // SAFETY: 调用方保证 `init` 执行期间不改变地址转换配置，也不会开启中断或并发。
        let result = unsafe { space.insert_page(STACK_PAGE, Access::ReadWrite, init) }?;
        Ok((space, result))
    }

    /// 检查按 4 KiB 对齐的用户虚拟地址半开区间 `[start, end)` 完整落在普通用户映射区内，避开零页、用户栈与栈保护页。
    ///
    /// # Errors
    ///
    /// 当 `start` 或 `end` 未按 4 KiB 对齐、区间为空或越出允许的普通用户映射范围时，返回 [`UserMappingError::InvalidUserAddress`]。
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

    /// 将按 4 KiB 对齐的用户虚拟页 `address` 映射到新分配的独占物理页框，将 `initial` 复制到页内字节偏移 `offset` 处，其余字节清零。
    ///
    /// # Errors
    ///
    /// 用户虚拟地址越界或未按 4 KiB 对齐时返回 [`UserMappingError::InvalidUserAddress`]；
    /// `offset` 与 `initial.len()` 超出单页边界时返回 [`UserMappingError::InitialDataTooLarge`]；
    /// 目标虚拟页已有映射时返回 [`UserMappingError::AlreadyMapped`]；
    /// 当前寻址模式不支持操作时返回 [`UserMappingError::UnavailableInCurrentMode`]；
    /// 页框或堆空间不足时返回 [`UserMappingError::OutOfMemory`]；
    /// 底层映射校验失败时返回 [`UserMappingError::InvalidMapping`]。
    /// 失败时保留已有映射并回收本次新分配的页框。
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
        if offset > PAGE_SIZE || initial.len() > PAGE_SIZE - offset {
            return Err(UserMappingError::InitialDataTooLarge);
        }
        // SAFETY: 回调只向页内缓冲区复制数据，不改变地址转换配置，也不开启中断或并发。
        unsafe {
            self.insert_page(address, access, |buf| {
                if !initial.is_empty() {
                    buf[offset..][..initial.len()].copy_from_slice(initial);
                }
                Ok::<_, UserMappingError>(())
            })
        }
    }

    /// 临时激活地址空间；闭包退出后恢复原地址空间。
    ///
    /// # Errors
    ///
    /// 当前寻址模式不支持激活或硬件未接受目标分页配置时返回 [`UserMappingError::UnavailableInCurrentMode`]，并恢复原状态且不执行闭包。
    ///
    /// # Safety
    ///
    /// - 当前须为关中断的启动主核特权态，并满足 [`UserRoot::with_active`] 要求的硬件与平台前提。
    /// - 内核栈（含 Trap 保存现场）须位于内核镜像的启动栈，闭包访问的内核对象须在切换前后保持持续可访问。
    /// - 闭包不得绕过本接口改写地址转换配置、开启中断或并发、访问未映射的页表存储，也不得返回依赖临时活动映射的引用。
    pub(crate) unsafe fn with_active<R>(
        &self,
        run: impl FnOnce() -> R,
    ) -> Result<R, UserMappingError> {
        // SAFETY: 调用方保证栈、Trap 现场、闭包对象及执行环境符合后端契约。本地址空间持有用户叶页，保证其在闭包期间有效。
        unsafe { self.table.with_active(run) }
    }

    /// 检查目标页尚未映射，再分配、初始化并映射用户页框。
    ///
    /// # Safety
    ///
    /// `init` 执行期间不得修改地址转换配置，也不得开启中断或并发。
    unsafe fn insert_page<R, E: From<UserMappingError>>(
        &mut self,
        address: usize,
        access: Access,
        init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> Result<R, E>,
    ) -> Result<R, E> {
        self.table.ensure_unmapped(address)?;
        self.pages
            .try_reserve(1)
            .map_err(|_| UserMappingError::OutOfMemory)?;
        let mut page = self.frames.alloc().ok_or(UserMappingError::OutOfMemory)?;
        // SAFETY: 构造契约保证启动执行环境和真实 RAM；刚分配的 `page` 独占可按同地址访问的物理页。
        // 本函数的调用方保证 `init` 不改变地址转换配置，也不启用中断或并发。
        let result = unsafe { user_paging::init_zeroed_frame(&mut page, init) }?;
        self.table.map_user(address, &page, access)?;
        self.pages.push(page);
        Ok(result)
    }
}

#[cfg(feature = "ktest")]
#[path = "../ktest/mm/address_space/mod.rs"]
pub(crate) mod tests;
