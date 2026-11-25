//! 内存管理子系统：静态内核堆、启动期物理内存发现与物理页框分配器。

pub(crate) mod boot_memory;
// 用户页表后端目前仅在 RISC-V 实现；装载策略本身不依赖 ISA。
#[cfg(target_arch = "riscv64")]
pub(crate) mod address_space;
pub(crate) mod fdt;
pub(crate) mod frame;
pub(crate) mod heap;
pub(crate) mod regions;

/// 用户映射的访问权限；所有组合都满足 W^X。
#[cfg_attr(
    target_arch = "loongarch64",
    expect(dead_code, reason = "LoongArch 尚未接入用户分页")
)]
#[derive(Clone, Copy)]
pub(crate) enum Access {
    /// 只读。
    Read,
    /// 可读、可写。
    ReadWrite,
    /// 只允许取指。
    Execute,
    /// 可读、可取指。
    ReadExecute,
}

/// 用户地址空间的构建、映射或激活失败。
///
/// 地址空间与分页后端共用此分类；硬件页表的错误由后端转换。
#[cfg_attr(
    target_arch = "loongarch64",
    expect(dead_code, reason = "LoongArch 尚未接入用户分页")
)]
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum UserMappingError {
    /// 用户地址未对齐或超出允许的映射范围。
    #[error("user virtual address is unaligned or out of range")]
    InvalidUserAddress,
    /// 初始内容放不进目标页。
    #[error("initial data does not fit in the page")]
    InitialDataTooLarge,
    /// 目标页已有映射。
    #[error("user virtual page is already mapped")]
    AlreadyMapped,
    /// 当前寻址模式不支持此操作。
    #[error("user paging operation is unavailable in the current mode")]
    UnavailableInCurrentMode,
    /// 地址或权限不符合底层映射要求。
    #[error("address or permissions are not valid for the paging backend")]
    InvalidMapping,
    /// 无法分配页框或映射元数据。
    #[error("out of memory for frames or mapping metadata")]
    OutOfMemory,
}

// 仅 init 写入一次，此后不移动或替换分配器；所有访问限于关闭中断的启动主核。
// 分配器包含 RefCell；启用中断或多核前必须重新设计访问保护。
static mut FRAMES: Option<frame::FrameAllocator> = None;

/// 返回正常启动时建立的全局页框分配器。
///
/// 与其他内存管理操作一样，仅供关闭中断的启动主核使用。
///
/// # Panics
///
/// [`init`] 尚未成功完成时 panic。
#[cfg_attr(
    all(target_arch = "loongarch64", not(feature = "ktest")),
    expect(dead_code, reason = "LoongArch 正常启动只初始化分配器，不分配用户页框")
)]
pub fn frames() -> &'static frame::FrameAllocator {
    // SAFETY: 仅关闭中断的启动主核可以访问；init 只在发布引用前写入一次，此后分配器及其堆元数据在内核整个生命周期内有效。
    unsafe { (*core::ptr::addr_of!(FRAMES)).as_ref() }.expect("frame allocator not initialized")
}

/// 依次初始化内核堆、发现物理内存，并建立全局页框分配器。
///
/// 必须早于首次堆分配。成功后通过 [`frames`] 借用分配器；启动数据不回收。
/// 初始化失败后不支持重试，因为内核堆已经初始化。
/// 没有完整可用页时仍可成功初始化，分配器容量为零，由后续分配请求报告资源不足。
///
/// # Errors
///
/// 当平台启动数据、设备树内存描述不合法或超出支持范围，或者静态堆空间不足以分配页框位图及索引时返回错误。
///
/// # Panics
///
/// 当重复调用本函数、内核镜像（`skernel..ekernel`）未完整落在 RAM 区间内，或解析设备树期间堆内存耗尽时 panic。
///
/// # Safety
///
/// - 仅可在关闭中断的启动主核上调用一次，不支持多核并发或中断重入。
/// - `boot_arg` 必须是入口汇编按当前架构启动协议传入的参数；其指向的启动数据在初始化期间必须完整、可读且不被修改，
///   并且当前架构已建立访问该启动数据所需的地址映射（如 RISC-V 物理直通或 LoongArch `DMW0`）。
pub unsafe fn init(boot_arg: usize) -> Result<(), &'static str> {
    heap::init();

    // SAFETY: 调用方已保证 `boot_arg`、启动数据存储及当前架构的早期映射有效。
    let memory = unsafe { crate::arch::boot::memory(boot_arg) }?;
    let map = discover(memory)?;

    let frames = frame::FrameAllocator::new(map)?;

    log::info!(
        "physical frames: total={}, available={}",
        frames.total(),
        frames.available()
    );

    // SAFETY: 调用方保证仅关闭中断的启动主核运行；heap::init 拒绝重复初始化，此前尚未发布分配器引用。写入后分配器始终留在原处。
    unsafe { core::ptr::addr_of_mut!(FRAMES).write(Some(frames)) };
    Ok(())
}

/// 将内核镜像自身占用的物理区间加入保留列表，并生成归一化后的 [`regions::MemoryMap`]。
///
/// 平台层在构造 `memory` 时已将 FDT 本体、EFI 启动块及 `initrd` 等列入 `reserved`。
///
/// # Errors
///
/// 当物理区间无效、内核链接符号边界异常或未描述任何 RAM 时返回错误。
///
/// # Panics
///
/// 当内核镜像物理区间未完整落在已描述的 RAM 区间内时 panic。
fn discover(mut memory: boot_memory::BootMemory) -> Result<regions::MemoryMap, &'static str> {
    use crate::arch::boot;
    use regions::MemoryMap;

    let kernel = boot::kernel_region()?;
    memory.reserved.push(kernel);
    let map = MemoryMap::new(memory.ram, memory.reserved, frame::PAGE_SIZE)?;

    assert!(
        map.ram()
            .iter()
            .any(|ram| ram.start <= kernel.start && kernel.end <= ram.end),
        "kernel outside described RAM"
    );

    for region in map.ram() {
        log::info!("RAM: [{:#x}, {:#x})", region.start, region.end);
    }
    for region in map.reserved() {
        log::info!("reserved: [{:#x}, {:#x})", region.start, region.end);
    }
    for region in map.usable() {
        log::info!("usable: [{:#x}, {:#x})", region.start, region.end);
    }
    Ok(map)
}
