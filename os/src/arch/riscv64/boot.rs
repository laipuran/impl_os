//! RV 固件启动参数：入口汇编将 a1 中的 FDT 地址传给共享入口。

use crate::arch::boot_data::MappedFdt;
use crate::mm::{fdt, regions::Region};

/// 在 bare 地址空间中读取固件设备树。
///
/// # Errors
///
/// FDT 地址、头部、结构或内存描述无效时返回错误。
///
/// # Safety
///
/// `boot_arg` 必须来自固件的 a1，指向完整、可读且不变的 FDT。
pub unsafe fn memory(boot_arg: usize) -> Result<fdt::BootMemory, &'static str> {
    // SAFETY: 调用者保证固件 FDT 可读；RV 当前物理地址与访问地址相同。
    let blob = unsafe { MappedFdt::from_raw(boot_arg, boot_arg, isize::MAX as usize) }?;
    let memory = blob.parse_memory()?;
    log::info!("FDT: physical={:#x}", boot_arg);
    Ok(memory)
}

/// 返回链接脚本覆盖的完整内核物理区间，包含启动栈和静态堆。
///
/// # Errors
///
/// 链接边界为空或顺序无效时返回错误。
pub fn kernel_region() -> Result<Region, &'static str> {
    extern "C" {
        static skernel: u8;
        static ekernel: u8;
    }
    let start = core::ptr::addr_of!(skernel) as usize;
    let end = core::ptr::addr_of!(ekernel) as usize;
    Region::new(
        start,
        end.checked_sub(start).ok_or("invalid kernel bounds")?,
    )
}
