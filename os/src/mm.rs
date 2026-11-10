//! 内核堆、启动期物理内存地图及物理页框分配。

pub(crate) mod fdt;
pub(crate) mod frame;
pub(crate) mod heap;
pub(crate) mod regions;

/// 依次初始化内核堆、发现物理内存，并返回管理可用页的分配器。
///
/// 必须早于首次堆分配。返回的分配器由调用者持有；启动数据不回收。
/// 初始化失败后不支持重试，因为内核堆已经初始化。
/// 没有完整可用页时返回空分配器，由后续分配请求报告资源不足。
///
/// # Errors
///
/// 启动内存描述无效或不受支持，或堆无法容纳页框位图及索引时返回错误。
///
/// # Panics
///
/// 重复初始化、内核未完整位于描述的 RAM 中，或启动信息解析期间堆分配失败时 panic。
///
/// # Safety
///
/// 仅在关闭中断的 boot CPU 上调用，不支持并发或中断重入。
/// `boot_arg` 必须是架构入口整理的平台启动参数；启动数据必须完整、可读且
/// 在发现期间不变，并已建立该架构要求的启动内存访问方式。
pub unsafe fn init(boot_arg: usize) -> Result<frame::FrameAllocator, &'static str> {
    heap::init();

    // SAFETY: 调用者保证平台参数、启动数据及架构访问方式有效。
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

/// 整理平台已解析的内存描述，并扣除内核占用。
///
/// 不访问原始启动存储；平台层已在输入中保留 FDT 本体和其他启动数据。
///
/// # Errors
///
/// 物理区间或内核链接边界无效，或未描述 RAM 时返回错误。
///
/// # Panics
///
/// 内核物理地址区间未完整包含在描述的 RAM 中时 panic。
fn discover(mut memory: fdt::BootMemory) -> Result<regions::MemoryMap, &'static str> {
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
