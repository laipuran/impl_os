//! Sv39 的 4 KiB 页映射、页表页所有权及单核作用域激活。
//!
//! 页表独占并回收自己的各级表页，不拥有叶子映射指向的代码、数据或设备。
//! 仅支持物理页可通过同地址内核指针访问的环境，不提供高半区地址转换、
//! 大页、ASID 分配或多核 TLB shootdown。

use alloc::vec::Vec;
use core::ops::BitOr;

use crate::mm::frame::{Frame, FrameAllocator, PAGE_SIZE};

mod csr;
mod pte;

use pte::Entry;

/// 叶子页面的访问权限；可用 `|` 组合，映射时校验 RISC-V 的合法编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions(usize);

impl Permissions {
    /// 允许读取。
    pub const READ: Self = Self(1 << 1);
    /// 允许写入；必须同时设置 [`Self::READ`]。
    pub const WRITE: Self = Self(1 << 2);
    /// 允许取指。
    pub const EXECUTE: Self = Self(1 << 3);
    /// 允许用户态访问；未设置时仅供监管态使用。
    pub const USER: Self = Self(1 << 4);

    const MASK: usize = Self::READ.0 | Self::WRITE.0 | Self::EXECUTE.0 | Self::USER.0;

    fn is_valid(self) -> bool {
        let readable = self.0 & Self::READ.0 != 0;
        let writable = self.0 & Self::WRITE.0 != 0;
        let executable = self.0 & Self::EXECUTE.0 != 0;
        readable || (executable && !writable)
    }
}

impl BitOr for Permissions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// 页表操作失败的原因；错误不会改变已有映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 虚拟地址不是合法的 Sv39 符号扩展地址。
    InvalidVirtualAddress,
    /// 映射或解除映射的地址未按 4 KiB 对齐。
    UnalignedAddress,
    /// 物理地址超出 56 位编码范围，或页表页不能表示为非空 Sv39 同地址指针。
    InvalidPhysicalAddress,
    /// 没有读/执行权限，或要求只写而不可读。
    InvalidPermissions,
    /// 目标虚拟页已有映射；不隐式覆盖。
    AlreadyMapped,
    /// 目标虚拟页没有映射。
    NotMapped,
    /// 页框或页表所有权记录的堆空间不足。
    OutOfMemory,
}

/// 软件查表结果；包含输入地址的页内偏移，不模拟 CPU 的特权级检查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Translation {
    /// 翻译后的物理字节地址，不是可直接解引用的内核指针。
    pub physical_address: usize,
    /// 叶子项上的权限，不包含内部管理的 A/D 位。
    pub permissions: Permissions,
}

/// 独占所有页表页，借用页框分配器的 Sv39 页表。
///
/// 表项通过物理地址直接遍历；`tables` 仅负责所有权和释放，不参与查表。
/// 不导入外部表项，不暴露可修改的表页指针，保证上级有效项始终指向自有表页。
pub struct PageTable<'a> {
    frames: &'a FrameAllocator,
    root: Frame<'a>,
    tables: Vec<Frame<'a>>,
}

#[cfg_attr(not(feature = "ktest"), allow(dead_code))] // 用户地址空间接入前由 ktest 使用。
impl<'a> PageTable<'a> {
    /// 分配并清零根页表；初始没有任何映射。
    ///
    /// # Errors
    ///
    /// 无空闲页框时返回 [`Error::OutOfMemory`]；页框地址超出编码或同地址指针范围时
    /// 返回 [`Error::InvalidPhysicalAddress`]，两者均归还已申请资源。
    ///
    /// # Safety
    ///
    /// 仅在关闭中断的 boot CPU 上使用。`frames` 必须管理真实且独占的可用 RAM，
    /// 不是合成记账区间。整个页表生命周期内，从该分配器获得的每个页表页
    /// 都必须能以非空物理地址作为内核指针读写，不与其他存活对象重叠；
    /// 页表页不得通过其他别名被修改或暴露为用户可写页面。
    pub unsafe fn new(frames: &'a FrameAllocator) -> Result<Self, Error> {
        // SAFETY: 调用者保证此分配器提供可直接访问的独占物理 RAM。
        let root = unsafe { allocate_table(frames) }?;
        Ok(Self {
            frames,
            root,
            tables: Vec::new(),
        })
    }

    /// 建立一个 4 KiB 映射，按需创建中间表；不访问目标物理页。
    ///
    /// 映射不持有目标页的所有权。只记录地址并不要求目标已分配，
    /// 但激活前必须满足 [`Self::with_active`] 的有效性和内存隔离要求。
    ///
    /// # Errors
    ///
    /// 地址无效、未对齐、权限非法、虚拟页已映射或资源不足时返回错误。
    /// 失败时不留下部分映射或额外的页表页。
    pub fn map(
        &mut self,
        virtual_address: usize,
        physical_address: usize,
        permissions: Permissions,
    ) -> Result<(), Error> {
        let indices = page_indices(virtual_address)?;
        if physical_address % PAGE_SIZE != 0 {
            return Err(Error::UnalignedAddress);
        }
        if !pte::valid_physical_address(physical_address) {
            return Err(Error::InvalidPhysicalAddress);
        }
        if !permissions.is_valid() {
            return Err(Error::InvalidPermissions);
        }
        let leaf = Entry::leaf(physical_address, permissions);
        let mut table = self.root.start_address();
        for level in 0..2 {
            let entry = self.read_entry(table, indices[level]);
            if !entry.is_valid() {
                return self.add_branch(table, indices[level], &indices[level + 1..], leaf);
            }
            debug_assert!(!entry.is_leaf(), "only 4 KiB leaves are constructed");
            table = entry.physical_address();
        }
        if self.read_entry(table, indices[2]).is_valid() {
            return Err(Error::AlreadyMapped);
        }
        self.write_entry(table, indices[2], leaf);
        Ok(())
    }

    /// 移除一个 4 KiB 映射，并立即回收空的中间表；根页表保留。
    ///
    /// 返回原物理页起始地址和权限，不释放或清零目标页面。
    ///
    /// # Errors
    ///
    /// 虚拟地址无效、未对齐或没有映射时返回错误。
    pub fn unmap(&mut self, virtual_address: usize) -> Result<Translation, Error> {
        let indices = page_indices(virtual_address)?;
        let path = self.walk(indices)?;
        let entry = self.read_entry(path[2], indices[2]);
        self.write_entry(path[2], indices[2], Entry::EMPTY);
        for level in (1..=2).rev() {
            if (0..512).any(|index| self.read_entry(path[level], index).is_valid()) {
                break;
            }
            self.write_entry(path[level - 1], indices[level - 1], Entry::EMPTY);
            // 只有空表回收需要查找所有权记录；正常查表直接沿硬件表项访问。
            let owner = self
                .tables
                .iter()
                .position(|frame| frame.start_address() == path[level])
                .expect("page table frame ownership");
            drop(self.tables.swap_remove(owner));
        }
        Ok(Translation {
            physical_address: entry.physical_address(),
            permissions: entry.permissions(),
        })
    }

    /// 软件遍历一个虚拟字节地址，保留页内偏移，不访问目标页或修改 A/D。
    ///
    /// # Errors
    ///
    /// 非 Sv39 地址或缺失映射时返回错误。
    pub fn translate(&self, virtual_address: usize) -> Result<Translation, Error> {
        let indices = pte::indices(virtual_address)?;
        let path = self.walk(indices)?;
        let entry = self.read_entry(path[2], indices[2]);
        Ok(Translation {
            physical_address: entry.physical_address() | (virtual_address & (PAGE_SIZE - 1)),
            permissions: entry.permissions(),
        })
    }

    /// 在本核临时激活页表，执行闭包后恢复原 `satp`；两次切换均全量刷新 TLB。
    ///
    /// 使用 ASID 0；共享借用阻止作用域内通过本接口修改或销毁活动页表。
    /// 返回值不延长临时映射的有效期，不能借此返回依赖该映射的引用。
    ///
    /// # Safety
    ///
    /// 必须在关闭中断的单个 boot CPU 的 S-mode 调用；硬件支持 Sv39。
    /// 新旧映射必须保持当前执行代码、栈、Rust 对象、trap 入口及其依赖地址
    /// 有效且指向同一存储，UART 等使用中的 MMIO 也须可访问。新映射必须
    /// 满足 Rust 别名规则，不允许用户修改内核或页表存储；所有叶子页和原页表
    /// 必须在整个作用域内有效。闭包不得自行修改 satp、活动表项或启用并发。
    /// 闭包中的裸地址访问和用户执行仍须分别满足各自的安全要求。
    pub unsafe fn with_active<R>(&self, run: impl FnOnce() -> R) -> R {
        // SAFETY: 调用者保证两次切换及闭包执行期间的映射和单核约束。
        let _restore = unsafe { csr::RestoreSatp::switch(self.root.start_address()) };
        run()
    }

    fn walk(&self, indices: [usize; 3]) -> Result<[usize; 3], Error> {
        let mut path = [self.root.start_address(); 3];
        for level in 0..3 {
            let entry = self.read_entry(path[level], indices[level]);
            if !entry.is_valid() {
                return Err(Error::NotMapped);
            }
            if level < 2 {
                debug_assert!(!entry.is_leaf(), "only 4 KiB leaves are constructed");
                path[level + 1] = entry.physical_address();
            }
        }
        Ok(path)
    }

    fn add_branch(
        &mut self,
        parent: usize,
        index: usize,
        indices: &[usize],
        leaf: Entry,
    ) -> Result<(), Error> {
        self.tables
            .try_reserve(indices.len())
            .map_err(|_| Error::OutOfMemory)?;
        let mut pending = [None, None];
        for slot in pending.iter_mut().take(indices.len()) {
            // SAFETY: new 的调用者保证分配器的实际 RAM 和同地址访问约束。
            *slot = Some(unsafe { allocate_table(self.frames) }?);
        }
        let first = pending[0].as_ref().unwrap().start_address();
        // 全部分配成功后才发布上级表项；分配失败由局部 Frame 自动回收。
        for level in 0..indices.len() {
            let table = pending[level].as_ref().unwrap().start_address();
            let entry = if level + 1 == indices.len() {
                leaf
            } else {
                Entry::branch(pending[level + 1].as_ref().unwrap().start_address())
            };
            self.write_entry(table, indices[level], entry);
        }
        self.tables.extend(pending.into_iter().flatten());
        self.write_entry(parent, index, Entry::branch(first));
        Ok(())
    }

    // table 仅来自自有根页、沿自有表项的遍历或尚未发布的独占新页；index < 512。
    fn read_entry(&self, table: usize, index: usize) -> Entry {
        debug_assert!(index < 512);
        // SAFETY: 上述私有遍历不变量保证地址有效；new 保证同地址内核访问。
        // 活动期间不修改表页，叶子 A/D 已置位，不依赖硬件写回。
        unsafe { (table as *const Entry).add(index).read() }
    }

    fn write_entry(&mut self, table: usize, index: usize, entry: Entry) {
        debug_assert!(index < 512);
        // SAFETY: 与 read_entry 相同的地址不变量；独占借用排除作用域激活，
        // 因而没有硬件或其他调用者同时使用正在修改的页表。
        unsafe { (table as *mut Entry).add(index).write(entry) };
    }
}

fn page_indices(address: usize) -> Result<[usize; 3], Error> {
    let indices = pte::indices(address)?;
    if address % PAGE_SIZE != 0 {
        return Err(Error::UnalignedAddress);
    }
    Ok(indices)
}

/// 分配并清零尚未发布的表页，失败时由局部句柄回收。
///
/// # Safety
///
/// 分配器提供的页框必须是独占 RAM，可通过同地址内核指针完整写入。
unsafe fn allocate_table(frames: &FrameAllocator) -> Result<Frame<'_>, Error> {
    let frame = frames.alloc().ok_or(Error::OutOfMemory)?;
    let address = frame.start_address();
    if address == 0 || !pte::valid_physical_address(address) || pte::indices(address).is_err() {
        return Err(Error::InvalidPhysicalAddress);
    }
    // SAFETY: 调用者保证分配的真实 RAM 可同地址访问；frame 独占完整页面。
    // 先清零再发布，避免把上一个使用者的数据解释为有效表项。
    unsafe { (address as *mut u8).write_bytes(0, PAGE_SIZE) };
    Ok(frame)
}

#[cfg(feature = "ktest")]
#[path = "../../../ktest/arch/riscv64/paging/mod.rs"]
pub(crate) mod tests;
