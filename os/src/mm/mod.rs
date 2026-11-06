//! 内核堆及启动期物理内存地图；尚不分配物理页框。

pub(crate) mod fdt;
pub(crate) mod heap;
pub(crate) mod regions;

/// 初始化内核内存管理，必须早于首次堆分配。
///
/// 仅在关闭中断的 boot CPU 上调用，不支持并发分配或中断上下文中的分配。
///
/// # Panics
///
/// 重复初始化时 panic。
pub fn init() {
    heap::init();
}

/// 通过架构启动信息读取内存布局，并扣除所有已知启动占用。
///
/// 必须在堆初始化后调用；设备树本身保留到关机，不回收启动数据。
///
/// # Errors
///
/// 设备树无效、包含不支持的内存描述，或没有完整可用页时返回错误。
///
/// # Safety
///
/// `boot_arg` 必须是架构入口整理的平台启动参数；对应启动数据必须完整、
/// 可读且不变，并已建立该架构要求的启动内存访问方式。
pub unsafe fn discover(boot_arg: usize) -> Result<regions::MemoryMap, &'static str> {
    use crate::arch::boot;
    use regions::MemoryMap;

    // SAFETY: 调用者保证参数来源、启动数据有效期及架构访问方式。
    let mut memory = unsafe { boot::memory(boot_arg) }?;
    let kernel = boot::kernel_region()?;
    memory.reserved.push(kernel);
    let map = MemoryMap::new(memory.ram, memory.reserved, 4096)?;
    if !map
        .ram
        .iter()
        .any(|ram| ram.start <= kernel.start && kernel.end <= ram.end)
    {
        return Err("kernel outside described RAM");
    }
    for region in &map.ram {
        log::info!("RAM: [{:#x}, {:#x})", region.start, region.end);
    }
    for region in &map.reserved {
        log::info!("reserved: [{:#x}, {:#x})", region.start, region.end);
    }
    for region in &map.usable {
        log::info!("usable: [{:#x}, {:#x})", region.start, region.end);
    }
    Ok(map)
}
