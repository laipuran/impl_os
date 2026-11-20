//! 支持同步任务的用户地址空间。用户态与内核态使用同一个根，只有运行作用域切换 `satp`。

use alloc::vec::Vec;
use core::{arch::asm, ptr};

use super::{csr, Error as PagingError, PageTable, Permissions};
use crate::mm::frame::{Frame, PAGE_SIZE};
use crate::task::{ExitStatus, Task};

use super::super::UART_BASE;

// 用户区结束于第一个内核设备映射之前，最末一页留给向下增长的栈。
const STACK_PAGE: usize = UART_BASE - PAGE_SIZE;
const STACK_GUARD: usize = STACK_PAGE - PAGE_SIZE;
const SV39_LOW_END: usize = 1 << 38;
const RX: Permissions = Permissions(Permissions::READ.0 | Permissions::EXECUTE.0);
const RW: Permissions = Permissions(Permissions::READ.0 | Permissions::WRITE.0);

/// 用户页的用途决定其访问权限。
pub enum PageKind {
    /// 可读、可执行的代码页。
    Code,
    /// 可读、可写的数据页。
    Data,
}

/// 构建或激活用户地址空间失败的原因。
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// 操作需要从启动时的 `satp = Bare` 地址空间执行。
    RequiresBare,
    /// 地址未按页对齐，或落在普通用户页范围之外。
    InvalidUserAddress,
    /// 初始内容超过一页。
    InitialDataTooLarge,
    /// 页表或物理页框操作失败。
    Paging(PagingError),
}

impl From<PagingError> for Error {
    fn from(error: PagingError) -> Self {
        Self::Paging(error)
    }
}

/// 独占一个 Sv39 根和用户叶页；内核和设备映射只借用既有存储。
///
/// 当前支持启动主核上的同步执行。保留零页、一个栈保护页及一页用户栈；
/// 普通用户页可以放在零页与栈保护页之间。
pub struct UserAddressSpace {
    // 按声明顺序先销毁页表，再归还用户叶页。
    table: PageTable<'static>,
    pages: Vec<Frame<'static>>,
}

impl UserAddressSpace {
    /// 创建一个地址空间，建立内核映射，分配并清零一页用户栈。
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
    pub unsafe fn new() -> Result<Self, Error> {
        require_bare()?;
        let frames = crate::mm::frames();
        // SAFETY: 调用方保证真实的独占 RAM；构建期间为 Bare，激活前会映射所有表页。
        let table = unsafe { PageTable::new(frames) }?;
        let mut space = Self {
            table,
            pages: Vec::new(),
        };
        space.map_kernel()?;
        space.insert_page(STACK_PAGE, RW, &[])?;
        Ok(space)
    }

    /// 返回初始用户栈指针，栈向低地址增长。
    pub const fn stack_top(&self) -> usize {
        STACK_PAGE + PAGE_SIZE
    }

    /// 分配并映射一页普通用户内存，先清零再复制初始内容。
    ///
    /// # Errors
    ///
    /// 页地址不在普通用户区或未对齐、内容超长、地址已映射、当前不是 Bare
    /// 或资源不足时返回错误。失败保留所有已有用户映射。
    pub fn map_page(
        &mut self,
        address: usize,
        kind: PageKind,
        initial: &[u8],
    ) -> Result<(), Error> {
        require_bare()?;
        if !(PAGE_SIZE..STACK_GUARD).contains(&address) || address % PAGE_SIZE != 0 {
            return Err(Error::InvalidUserAddress);
        }
        if initial.len() > PAGE_SIZE {
            return Err(Error::InitialDataTooLarge);
        }
        if self.table.translate(address).is_ok() {
            return Err(PagingError::AlreadyMapped.into());
        }
        let permissions = match kind {
            PageKind::Code => RX,
            PageKind::Data => RW,
        };
        self.insert_page(address, permissions, initial)
    }

    /// 激活本地址空间，同步运行任务，并在正常退出或用户故障后恢复原 `satp`。
    ///
    /// 用户态 Trap 与 `sret` 沿用活动根；退出执行作用域时才恢复 Bare。
    ///
    /// # Errors
    ///
    /// 当前不是 Bare，或无法为表页建立监管态映射时返回错误，任务尚未开始运行。
    ///
    /// # Safety
    ///
    /// 当前须为关闭中断的启动主核，已安装 Trap 入口，且无其他用户态切换。
    /// 任务的 PC 和栈须符合 RISC-V 对齐要求。地址空间与任务对象须位于持续
    /// 映射的内核栈、静态数据或静态堆中。需要执行返回时，用户程序须最终
    /// 退出或触发终止任务的故障；内核不提供抢占。
    pub unsafe fn run(&mut self, task: &mut Task) -> Result<ExitStatus, Error> {
        require_bare()?;
        self.map_table_pages()?;
        // SAFETY: 当前仍在 S-mode，用户代码在 Bare 下写入；首次取指前同步本核指令缓存。
        unsafe { asm!("fence.i", options(nostack)) };
        // SAFETY: 内核镜像、栈、UART 和表页保持同地址监管态映射；用户映射仅指向
        // 本空间持有的独占页。调用方保证任务对象可访问、Trap 就绪且无并发切换。
        Ok(unsafe { self.table.with_active(|| task.run()) })
    }

    fn insert_page(
        &mut self,
        address: usize,
        permissions: Permissions,
        initial: &[u8],
    ) -> Result<(), Error> {
        self.pages
            .try_reserve(1)
            .map_err(|_| PagingError::OutOfMemory)?;
        let page = self.table.frames.alloc().ok_or(PagingError::OutOfMemory)?;
        let physical = page.start_address();
        if !(UART_BASE..SV39_LOW_END).contains(&physical) {
            return Err(PagingError::InvalidPhysicalAddress.into());
        }
        // SAFETY: 操作入口已验证 Bare，分配器提供独占真实页；私有调用者保证初始
        // 内容不超过一页，存活的源切片与这个新页框不重叠。
        unsafe {
            let destination = physical as *mut u8;
            destination.write_bytes(0, PAGE_SIZE);
            ptr::copy_nonoverlapping(initial.as_ptr(), destination, initial.len());
        }
        self.table
            .map(address, physical, permissions | Permissions::USER)?;
        self.pages.push(page);
        Ok(())
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

    fn map_table_pages(&mut self) -> Result<(), PagingError> {
        self.map_table_page(self.table.root.start_address())?;
        // 建立同地址映射本身也可能新增中间表，直到覆盖所有表页才允许激活。
        let mut index = 0;
        while index < self.table.tables.len() {
            self.map_table_page(self.table.tables[index].start_address())?;
            index += 1;
        }
        Ok(())
    }

    fn map_table_page(&mut self, address: usize) -> Result<(), PagingError> {
        if !(UART_BASE..SV39_LOW_END).contains(&address) {
            return Err(PagingError::InvalidPhysicalAddress);
        }
        match self.table.translate(address) {
            Ok(entry) if entry.physical_address == address && entry.permissions == RW => Ok(()),
            Ok(_) => Err(PagingError::AlreadyMapped),
            Err(PagingError::NotMapped) => self.table.map(address, address, RW),
            Err(error) => Err(error),
        }
    }
}

fn require_bare() -> Result<(), Error> {
    if csr::satp() == 0 {
        Ok(())
    } else {
        Err(Error::RequiresBare)
    }
}

#[cfg(feature = "ktest")]
#[path = "../../../ktest/arch/riscv64/user_space/mod.rs"]
pub(crate) mod tests;
