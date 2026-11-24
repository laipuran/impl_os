//! 独占用户页及其映射；构建期间地址空间保持非活动。
//!
//! [`AddressSpace`] 声明字段时确保先销毁底层页表、再归还映射所引用的用户叶页，避免析构期间产生指向已释放物理页的悬垂表项。

use alloc::vec::Vec;
use core::convert::Infallible;

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

/// 构建用户地址空间失败的原因。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BuildError<E> {
    /// 地址空间构建或页映射失败。
    Mapping(UserMappingError),
    /// 初始化回调失败。
    Init(E),
}

impl<'a> AddressSpace<'a> {
    /// 初始用户栈映射的用户虚拟地址半开上界（不包含该地址）。
    pub(crate) const STACK_TOP: usize = STACK_PAGE + PAGE_SIZE;

    /// 使用指定分配器构建带一页清零用户栈的地址空间。
    ///
    /// # Errors
    ///
    /// 返回 [`Self::with_stack_in`] 的映射错误。
    ///
    /// # Safety
    ///
    /// 调用方须满足 [`Self::with_stack_in`] 的执行环境和分配器前提。
    #[cfg(feature = "ktest")]
    pub(crate) unsafe fn new_in(frames: &'a FrameAllocator) -> Result<Self, UserMappingError> {
        // SAFETY: 调用方保证执行环境与分配器有效；空回调不改变运行状态。
        let (space, ()) = unsafe { Self::with_stack_in(frames, |_| Ok::<_, Infallible>(())) }
            .map_err(|error| match error {
                BuildError::Mapping(error) => error,
                BuildError::Init(never) => match never {},
            })?;
        Ok(space)
    }

    /// 构建地址空间和一页用户栈，返回地址空间与栈初始化结果。
    ///
    /// 回调在已清零的物理栈页上原位执行，缓冲区对应以 [`Self::STACK_TOP`] 为上界的最后一页。
    ///
    /// # Errors
    ///
    /// 寻址模式不受支持、资源不足或映射校验失败时返回 [`BuildError::Mapping`]；
    /// 栈初始化回调失败时返回 [`BuildError::Init`]。
    /// 失败时回收本次构建已申请的全部页框。
    ///
    /// # Safety
    ///
    /// - 仅在关中断的启动主核特权态调用和使用，并满足 [`UserRoot::new`] 的当前架构前提。
    /// - 分配器在借用期间须提供可由当前架构内核指针独占读写的真实 RAM 页框。
    /// - 栈初始化回调执行期间不得修改地址转换配置，也不得开启中断或并发。
    pub(crate) unsafe fn with_stack_in<R, E>(
        frames: &'a FrameAllocator,
        init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> Result<R, E>,
    ) -> Result<(Self, R), BuildError<E>> {
        // SAFETY: 调用方保证分配器和启动执行环境满足构建要求。
        let table = unsafe { UserRoot::new(frames) }.map_err(BuildError::Mapping)?;
        let mut space = Self {
            table,
            pages: Vec::new(),
            frames,
        };
        // SAFETY: 调用方保证回调执行期间不改变地址转换配置，也不会开启中断或并发。
        let result = unsafe { space.insert_page_with(STACK_PAGE, Access::ReadWrite, init) }?;
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
            self.insert_page_with(address, access, |buf| {
                if !initial.is_empty() {
                    buf[offset..][..initial.len()].copy_from_slice(initial);
                }
                Ok::<_, Infallible>(())
            })
        }
        .map_err(|error| match error {
            BuildError::Mapping(error) => error,
            BuildError::Init(never) => match never {},
        })
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
    unsafe fn insert_page_with<R, E>(
        &mut self,
        address: usize,
        access: Access,
        init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> Result<R, E>,
    ) -> Result<R, BuildError<E>> {
        self.table
            .ensure_unmapped(address)
            .map_err(BuildError::Mapping)?;
        self.pages
            .try_reserve(1)
            .map_err(|_| BuildError::Mapping(UserMappingError::OutOfMemory))?;
        let mut page = self
            .frames
            .alloc()
            .ok_or(BuildError::Mapping(UserMappingError::OutOfMemory))?;
        // SAFETY: 构造契约保证启动执行环境和真实 RAM；刚分配的页框可由内核独占读写。
        // 调用方保证 `init` 不改变地址转换配置，也不启用中断或并发。
        let result = unsafe { init_zeroed_frame(&mut page, init) }
            .map_err(BuildError::Mapping)?
            .map_err(BuildError::Init)?;
        self.table
            .map_user(address, &page, access)
            .map_err(BuildError::Mapping)?;
        self.pages.push(page);
        Ok(result)
    }
}

/// 清零独占物理页框，并在回调期间借出整页可变缓冲区。
///
/// # Errors
///
/// 当架构后端无法提供当前模式下的内核指针时返回对应的 [`UserMappingError`]，不执行回调。
///
/// # Safety
///
/// - 仅在关中断的启动主核特权态调用。
/// - `frame` 须对应真实、独占的整页 RAM，可通过 [`user_paging::frame_ptr`] 返回的指针完整读写，且不与任何存活的引用重叠。
/// - `init` 执行期间不得修改地址转换配置，也不得开启中断或并发。
unsafe fn init_zeroed_frame<R>(
    frame: &mut Frame<'_>,
    init: impl FnOnce(&mut [u8; PAGE_SIZE]) -> R,
) -> Result<R, UserMappingError> {
    let buf = user_paging::frame_ptr(frame)?;
    // SAFETY: Frame 保证整页对齐，后端已确认当前寻址模式可用且地址非零、可编码。
    // 调用方保证整页可通过该指针读写、无存活引用重叠，且回调不改变寻址或并发状态；先清零再建立本次回调期内的可变引用。
    unsafe {
        buf.write_bytes(0, 1);
        Ok(init(&mut *buf))
    }
}

#[cfg(feature = "ktest")]
#[path = "../ktest/mm/address_space.rs"]
pub(crate) mod tests;
