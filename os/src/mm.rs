//! 内存管理子系统：静态内核堆、启动期物理内存发现与物理页框分配器。

pub(crate) mod boot_memory;
pub(crate) mod fdt;
pub(crate) mod frame;
pub(crate) mod heap;
pub(crate) mod regions;

/// 初始化内核静态堆、解析启动内存布局，并构建管理可用物理页的 [`frame::FrameAllocator`]。
///
/// 必须在首次堆分配前调用，且只允许调用一次（即使后续内存发现报错，静态堆也已初始化，不支持重试）。
/// 若扣除保留区并按页对齐后没有任何完整的可用物理页，仍会返回一个总容量为 0 的分配器，
/// 由后续的页框分配调用按资源耗尽处理。
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
pub unsafe fn init(boot_arg: usize) -> Result<frame::FrameAllocator, &'static str> {
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

    Ok(frames)
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
