//! Sv39 用户根页表：建立平台所需映射并管理作用域激活。

use core::{arch::asm, ptr};

use super::{csr, pte, Error as PagingError, PageTable, Permissions};
use crate::mm::{
    frame::{Frame, PAGE_SIZE},
    UserMappingError,
};

use super::super::UART_BASE;

// 用户区结束于第一个内核设备映射之前。
pub(crate) const USER_END: usize = UART_BASE;
const RX: Permissions = Permissions(Permissions::READ.0 | Permissions::EXECUTE.0);
const RW: Permissions = Permissions(Permissions::READ.0 | Permissions::WRITE.0);

impl From<PagingError> for UserMappingError {
    fn from(error: PagingError) -> Self {
        match error {
            PagingError::AlreadyMapped => Self::AlreadyMapped,
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
pub(crate) struct Sv39UserRoot {
    table: PageTable<'static>,
}

impl Sv39UserRoot {
    /// 创建用户页表并建立内核映射。
    ///
    /// # Errors
    ///
    /// 当前地址转换不是 Bare，或页框、页表资源不足时返回错误。
    ///
    /// # Panics
    ///
    /// 内存管理尚未初始化时 panic。
    ///
    /// # Safety
    ///
    /// 仅在关闭中断的启动主核上调用。启动页框分配器必须管理可用物理 RAM；
    /// 在 Bare 下分配的页框必须可通过同地址内核指针独占读写。
    pub unsafe fn new() -> Result<Self, UserMappingError> {
        require_bare()?;
        let frames = crate::mm::frames();
        // SAFETY: 调用方保证独占 RAM；构建和软件查表时由 Bare 提供同地址访问。
        let table = unsafe { PageTable::new(frames) }?;
        let mut space = Self { table };
        space.map_kernel()?;
        Ok(space)
    }

    /// 映射已有用户页。调用方须保留该页的所有权直至页表销毁。
    pub(crate) fn map_user(
        &mut self,
        address: usize,
        page: &Frame<'static>,
        permissions: Permissions,
    ) -> Result<(), UserMappingError> {
        require_bare()?;
        Ok(self.table.map(
            address,
            page.start_address(),
            permissions | Permissions::USER,
        )?)
    }

    /// 激活本页表，执行闭包后恢复原 `satp`。
    ///
    /// 激活只同步指令并切换根，不创建映射。用户态 Trap 与 `sret` 沿用活动根；
    /// 退出执行作用域时恢复 Bare。
    ///
    /// # Errors
    ///
    /// 当前不是 Bare 时返回错误，闭包不执行。
    ///
    /// # Safety
    ///
    /// 当前须为关闭中断的启动主核。调用者当前的监管态栈（包括 Trap 保存现场）
    /// 须位于内核镜像的启动栈，两次切换均保持同地址可访问。闭包使用的内核对象
    /// 须同样持续可访问，且不得通过同地址指针访问未映射的表页。用户映射只能
    /// 指向调用方持有的独占页，并在闭包执行期间有效。
    pub unsafe fn with_active<R>(
        &mut self,
        run: impl FnOnce() -> R,
    ) -> Result<R, UserMappingError> {
        require_bare()?;
        // SAFETY: 当前仍在 S-mode，用户代码在 Bare 下写入；首次取指前同步本核指令缓存。
        unsafe { asm!("fence.i", options(nostack)) };
        // SAFETY: 内核镜像与 UART 保持同地址监管态映射；调用方在镜像内的启动栈
        // 上执行，保证 Trap 现场和闭包对象可访问，并且不会软件访问未映射的表页。
        // 自有用户页在闭包结束前保持有效，CPU 直接通过物理地址遍历表页。
        Ok(unsafe { self.table.with_active(run) })
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
}

/// 分配一页可直接初始化的用户 RAM，并清零文件内容之外的区域。
pub(crate) fn allocate_user_page(
    offset: usize,
    initial: &[u8],
) -> Result<Frame<'static>, UserMappingError> {
    require_bare()?;
    assert!(offset <= PAGE_SIZE && initial.len() <= PAGE_SIZE - offset);
    let page = crate::mm::frames()
        .alloc()
        .ok_or(PagingError::OutOfMemory)?;
    let physical = page.start_address();
    // 叶页只在 Bare 下按物理地址初始化，激活后通过用户虚拟地址访问；
    // 物理地址须可编码且非空，与设备的同地址映射范围无关。
    if physical == 0 || !pte::valid_physical_address(physical) {
        return Err(PagingError::InvalidPhysicalAddress.into());
    }
    // SAFETY: 操作入口已验证 Bare 及页内范围，分配器提供独占真实页，
    // 存活的源切片与这个新页框不重叠。
    unsafe {
        let destination = physical as *mut u8;
        destination.write_bytes(0, PAGE_SIZE);
        ptr::copy_nonoverlapping(initial.as_ptr(), destination.add(offset), initial.len());
    }
    Ok(page)
}

impl Sv39UserRoot {
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
    if csr::satp() == 0 {
        Ok(())
    } else {
        Err(UserMappingError::UnavailableInCurrentMode)
    }
}

#[cfg(feature = "ktest")]
pub(crate) fn satp() -> usize {
    csr::satp()
}
