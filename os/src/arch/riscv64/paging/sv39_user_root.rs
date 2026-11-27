//! Sv39 用户根页表：建立平台所需映射并管理作用域激活。

use core::ptr;

use super::{is_bare, pte, Error as PagingError, PageTable, Permissions};
use crate::mm::{
    frame::{Frame, FrameAllocator, PAGE_SIZE},
    Access, UserMappingError,
};

use super::super::UART_BASE;

// 用户区结束于第一个内核设备映射之前。
pub(crate) const USER_END: usize = UART_BASE;
const RX: Permissions = Permissions(Permissions::READ.0 | Permissions::EXECUTE.0);
const RW: Permissions = Permissions(Permissions::READ.0 | Permissions::WRITE.0);

/// 返回 Bare 模式下与页框物理地址相同的整页内核裸指针，不访问内存。
///
/// 解引用前，调用方仍须保证整页可通过该指针访问，并满足独占访问条件。
///
/// # Errors
///
/// 当前不处于 Bare 模式时返回 [`UserMappingError::UnavailableInCurrentMode`]；
/// 地址为零或超出 Sv39 物理地址编码范围时返回 [`UserMappingError::InvalidMapping`]。
pub(crate) fn frame_ptr(frame: &Frame<'_>) -> Result<*mut [u8; PAGE_SIZE], UserMappingError> {
    require_bare()?;
    let physical = frame.start_address();
    if physical == 0 || !pte::valid_physical_address(physical) {
        return Err(UserMappingError::InvalidMapping);
    }
    Ok(physical as *mut [u8; PAGE_SIZE])
}

impl From<PagingError> for UserMappingError {
    fn from(error: PagingError) -> Self {
        match error {
            PagingError::AlreadyMapped => Self::AlreadyMapped,
            PagingError::ActivationFailed => Self::UnavailableInCurrentMode,
            PagingError::OutOfMemory => Self::OutOfMemory,
            PagingError::InvalidVirtualAddress
            | PagingError::UnalignedAddress
            | PagingError::InvalidPhysicalAddress
            | PagingError::InvalidPermissions
            | PagingError::NotMapped => Self::InvalidMapping,
        }
    }
}

/// 独占 Sv39 表页，借用内核、设备及用户叶页对应的存储。
pub(crate) struct Sv39UserRoot<'a> {
    table: PageTable<'a>,
}

impl<'a> Sv39UserRoot<'a> {
    /// 使用指定页框分配器创建用户页表并建立内核与 UART 的监管态同地址映射。
    ///
    /// # Errors
    ///
    /// 当前地址转换不是 Bare 时返回 [`UserMappingError::UnavailableInCurrentMode`]；
    /// 页框或堆空间不足时返回 [`UserMappingError::OutOfMemory`]；
    /// 底层页表地址校验失败时返回 [`UserMappingError::InvalidMapping`]。
    /// 失败时回收本次已申请的全部页框。
    ///
    /// # Safety
    ///
    /// - 仅在关闭中断的启动主核 S-mode 调用和使用。
    /// - `frames` 在借用期间须提供在 Bare 下可通过同地址内核指针独占读写的真实 RAM 页框。
    pub(crate) unsafe fn new(frames: &'a FrameAllocator) -> Result<Self, UserMappingError> {
        require_bare()?;
        // SAFETY: 调用方保证独占 RAM；构建和软件查表时由 Bare 提供同地址访问。
        let table = unsafe { PageTable::new(frames) }?;
        let mut space = Self { table };
        space.map_kernel()?;
        Ok(space)
    }

    /// 在 Bare 下检查按 4 KiB 对齐的用户虚拟地址 `address` 尚未建立映射，不申请页框或堆空间。
    ///
    /// # Errors
    ///
    /// 当前不是 Bare 时返回 [`UserMappingError::UnavailableInCurrentMode`]；
    /// 目标虚拟页已有映射时返回 [`UserMappingError::AlreadyMapped`]；
    /// 虚拟地址未按 4 KiB 对齐或不是合法 Sv39 地址时返回 [`UserMappingError::InvalidMapping`]。
    pub(crate) fn ensure_unmapped(&self, address: usize) -> Result<(), UserMappingError> {
        require_bare()?;
        if address % PAGE_SIZE != 0 {
            return Err(UserMappingError::InvalidMapping);
        }
        match self.table.translate(address) {
            Ok(_) => Err(UserMappingError::AlreadyMapped),
            Err(PagingError::NotMapped) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// 将按 4 KiB 对齐的用户虚拟地址 `address` 映射到已有用户物理页框 `page`。
    ///
    /// 调用方须保留 `page` 的所有权直至本页表销毁。
    ///
    /// # Errors
    ///
    /// 当前不是 Bare 时返回 [`UserMappingError::UnavailableInCurrentMode`]；
    /// 目标虚拟页已有映射时返回 [`UserMappingError::AlreadyMapped`]；
    /// 页框或堆空间不足时返回 [`UserMappingError::OutOfMemory`]；
    /// 地址或权限不符合底层页表要求时返回 [`UserMappingError::InvalidMapping`]。
    /// 失败时保留已有映射并回收本次新分配的中间表页。
    pub(crate) fn map_user(
        &mut self,
        address: usize,
        page: &Frame<'a>,
        access: Access,
    ) -> Result<(), UserMappingError> {
        require_bare()?;
        let permissions = match access {
            Access::Read => Permissions::READ,
            Access::ReadWrite => RW,
            Access::Execute => Permissions::EXECUTE,
            Access::ReadExecute => RX,
        };
        Ok(self.table.map(
            address,
            page.start_address(),
            permissions | Permissions::USER,
        )?)
    }

    /// 激活本页表，执行闭包后恢复原 `satp`。
    ///
    /// 激活只同步本核指令缓存并切换根页表，不创建或分配任何映射。
    /// 用户态 Trap 与 `sret` 沿用活动根；退出执行作用域时恢复 Bare 模式。
    ///
    /// # Errors
    ///
    /// 当前不是 Bare，或硬件未接受预期的 Sv39 配置时返回 [`UserMappingError::UnavailableInCurrentMode`]，并恢复原配置且不执行闭包。
    ///
    /// # Safety
    ///
    /// - 当前须为关中断的启动主核 S-mode，且硬件支持 Sv39。
    /// - 监管态栈（含 Trap 保存现场）须位于内核镜像的启动栈，闭包访问的内核对象须在切换前后保持同地址可访问。
    /// - 调用方持有的用户叶页在闭包执行期间须保持有效，且不得允许用户态访问内核或页表存储。
    /// - 闭包不得改写 `satp`、开启中断或并发、通过同地址指针访问未映射的表页，也不得返回依赖临时活动映射的引用。
    pub(crate) unsafe fn with_active<R>(
        &self,
        run: impl FnOnce() -> R,
    ) -> Result<R, UserMappingError> {
        require_bare()?;
        // 切换页表前，同步本核对用户代码的写入与取指。
        riscv::asm::fence_i();
        // SAFETY: 内核镜像与 UART 保持同地址监管态映射。调用方保证启动栈、闭包对象、用户叶页和执行环境持续有效。
        Ok(unsafe { self.table.with_active(run) }?)
    }

    #[cfg(feature = "ktest")]
    pub(crate) fn translate(&self, address: usize) -> Result<super::Translation, PagingError> {
        self.table.translate(address)
    }

    #[cfg(feature = "ktest")]
    pub(crate) fn root_address(&self) -> usize {
        self.table.root.start_address()
    }

    #[cfg(feature = "ktest")]
    pub(crate) fn table_page_addresses(&self) -> impl Iterator<Item = usize> + '_ {
        core::iter::once(self.root_address())
            .chain(self.table.tables.iter().map(Frame::start_address))
    }

    fn map_kernel(&mut self) -> Result<(), PagingError> {
        extern "C" {
            static skernel: u8;
            static srodata: u8;
            static sdata: u8;
            static ekernel: u8;
        }
        let text = ptr::addr_of!(skernel) as usize;
        let rodata = ptr::addr_of!(srodata) as usize;
        let data = ptr::addr_of!(sdata) as usize;
        let end = (ptr::addr_of!(ekernel) as usize).next_multiple_of(PAGE_SIZE);

        // 链接脚本给出页对齐的分界，BSS（含栈和静态堆）与 data 同为可写。
        for (start, end, permissions) in [
            (text, rodata, RX),
            (rodata, data, Permissions::READ),
            (data, end, RW),
        ] {
            for address in (start..end).step_by(PAGE_SIZE) {
                self.table.map(address, address, permissions)?;
            }
        }
        self.table.map(UART_BASE, UART_BASE, RW)
    }
}

fn require_bare() -> Result<(), UserMappingError> {
    if is_bare() {
        Ok(())
    } else {
        Err(UserMappingError::UnavailableInCurrentMode)
    }
}
