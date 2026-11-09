//! 内核堆及启动期物理内存地图。

pub(crate) mod fdt;
pub(crate) mod heap;
pub(crate) mod regions;

/// 依次初始化内核堆、发现物理内存，并返回内存地图。
///
/// 必须早于首次堆分配。返回的地图由调用者持有；启动数据不回收。
/// 初始化失败后不支持重试，因为内核堆已经初始化。
/// 没有完整可用页时返回的地图仍有效，其可用区间列表为空。
///
/// # Errors
///
/// 启动内存描述无效或不受支持时返回错误。
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
pub unsafe fn init(boot_arg: usize) -> Result<regions::MemoryMap, &'static str> {
    heap::init();
    // SAFETY: 调用者保证平台参数、启动数据及架构访问方式有效。
    unsafe { discover(boot_arg) }
}

/// 通过架构启动信息读取内存布局，并扣除所有已知启动占用。
///
/// 必须在堆初始化后调用；设备树本身保留到关机，不回收启动数据。
///
/// # Errors
///
/// 设备树无效、包含不支持的内存描述，或未描述 RAM 时返回错误。
///
/// # Panics
///
/// 内核物理地址区间未完整包含在描述的 RAM 中时 panic。
///
/// # Safety
///
/// `boot_arg` 必须是架构入口整理的平台启动参数；对应启动数据必须完整、
/// 可读且不变，并已建立该架构要求的启动内存访问方式。
unsafe fn discover(boot_arg: usize) -> Result<regions::MemoryMap, &'static str> {
    use crate::arch::boot;
    use regions::MemoryMap;

    // SAFETY: 调用者保证参数来源、启动数据有效期及架构访问方式。
    let mut memory = unsafe { boot::memory(boot_arg) }?;
    let kernel = boot::kernel_region()?;
    memory.reserved.push(kernel);
    let map = MemoryMap::new(memory.ram, memory.reserved, 4096)?;
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
