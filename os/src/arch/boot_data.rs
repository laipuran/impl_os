//! 读取架构已建立映射的启动数据，将裸地址访问与安全的 FDT 解析分开。

use crate::mm::{fdt, regions::Region};

/// 借用已映射的 FDT 存储；不代表其节点结构或内存描述已经通过解析。
pub(super) struct MappedFdt<'a> {
    bytes: &'a [u8],
    storage: Region,
}

impl MappedFdt<'_> {
    /// 检查头部和平台容量，并借用头部声明的完整 FDT。
    ///
    /// `physical` 仅记录物理占用，不用于解引用；`capacity` 是平台允许的最大长度。
    ///
    /// # Errors
    ///
    /// 地址、头部或物理存储区间无效，或总长度超过平台容量时返回错误。
    ///
    /// # Safety
    ///
    /// 通过地址检查且容量足够时，`mapped` 指向的头部必须可读。
    /// 如果头部声明的总长度通过校验且不超过 `capacity`，完整范围也必须可读。
    /// 这些字节须在返回值的借用期间保持有效且不变。用于真实启动内存管理时，调用者
    /// 还须保证 `physical` 是该映射对应的物理地址。
    pub(super) unsafe fn from_raw(
        physical: usize,
        mapped: usize,
        capacity: usize,
    ) -> Result<Self, &'static str> {
        if physical % 8 != 0 || mapped == 0 || mapped.checked_add(fdt::HEADER_SIZE).is_none() {
            return Err("invalid FDT address");
        }
        if capacity < fdt::HEADER_SIZE {
            return Err("FDT exceeds platform storage");
        }

        // SAFETY: 调用者保证头部可读且不变；已检查空地址、地址溢出和头部容量。
        let header = unsafe { core::slice::from_raw_parts(mapped as *const u8, fdt::HEADER_SIZE) };
        let size = fdt::total_size(header)?;
        if size > capacity || mapped.checked_add(size).is_none() {
            return Err("FDT exceeds platform storage");
        }
        let storage = Region::new(physical, size)?;

        // SAFETY: 调用者保证校验后的完整范围在返回值的借用期间可读且不变；总长度
        // 已限制为切片可表示的范围，且虚拟地址与物理区间均无溢出。
        let bytes = unsafe { core::slice::from_raw_parts(mapped as *const u8, size) };
        Ok(Self { bytes, storage })
    }

    /// 安全解析借用的字节，并将 FDT 本体的物理占用追加到启动保留区。
    ///
    /// # Errors
    ///
    /// FDT 节点结构或内存描述无效、不受支持时返回错误。
    pub(super) fn parse_memory(&self) -> Result<fdt::BootMemory, &'static str> {
        let mut memory = fdt::parse(self.bytes)?;
        // TODO: 这里我们不对 `FDT` 进行完全解析，暂时把 `FDT` 放在 `mm`，后续
        // 可以移出为单独模块，可以不保留 `FDT`。
        memory.reserved.push(self.storage);
        Ok(memory)
    }
}

#[cfg(feature = "ktest")]
pub(crate) mod tests;
