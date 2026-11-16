//! 架构层 FDT 原始内存借用封装：将裸地址边界检查与纯安全的切片解析解耦。

use crate::mm::{boot_memory::BootMemory, fdt, regions::Region};

/// 对已映射 FDT 字节切片及其物理占用区间的借用封装（构造成功仅代表头部与长度合法，尚未解析内部节点）。
pub(super) struct MappedFdt<'a> {
    bytes: &'a [u8],
    storage: Region,
}

impl MappedFdt<'_> {
    /// 校验 FDT 地址对齐、头部魔数与平台最大容量限制，并构造覆盖完整 FDT 的字节切片。
    ///
    /// `physical` 仅用于记录 FDT 本体占用的物理区间（不会直接解引用）；`mapped` 为当前可读取该 FDT 的访问地址；
    /// `capacity` 为平台允许该 FDT 占用的最大字节数。
    ///
    /// # Errors
    ///
    /// 当 `physical` 未按 8 字节对齐、`mapped` 为空或溢出、FDT 头部无效，或 `totalsize` 超出 `capacity` 时返回错误。
    ///
    /// # Safety
    ///
    /// - 当地址对齐检查通过且 `capacity >= fdt::HEADER_SIZE` 时，`mapped` 起至少 `fdt::HEADER_SIZE`（40 字节）必须可读。
    /// - 当头部校验通过且声明的总长度 `size <= capacity` 时，`mapped` 起共 `size` 字节在返回值的生命周期内必须持续有效、可读且不被修改。
    /// - 若用于启动期真实物理内存管理，调用者还需保证 `physical` 确实是 `mapped` 对应的物理起始地址。
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

        // SAFETY: 已校验非空、无地址溢出且容量足以容纳 40 字节头部；调用方保证头部内存可读且不变。
        let header = unsafe { core::slice::from_raw_parts(mapped as *const u8, fdt::HEADER_SIZE) };
        let size = fdt::total_size(header)?;
        if size > capacity || mapped.checked_add(size).is_none() {
            return Err("FDT exceeds platform storage");
        }
        let storage = Region::new(physical, size)?;

        // SAFETY: 已校验 `size <= capacity` 且虚拟/物理区间均无溢出；调用方保证完整 FDT 范围在借用期间可读且不变。
        let bytes = unsafe { core::slice::from_raw_parts(mapped as *const u8, size) };
        Ok(Self { bytes, storage })
    }

    /// 解析 FDT 中的物理内存布局，并将 FDT 本体占用的物理区间追加到保留列表中。
    ///
    /// # Errors
    ///
    /// 当 FDT 节点结构损坏或内存描述超出支持范围时返回错误。
    pub(super) fn parse_memory(&self) -> Result<BootMemory, &'static str> {
        let mut memory = fdt::parse(self.bytes)?;
        // TODO: 目前仅提取内存布局信息；后续若将完整设备树解析独立为外设模块，可按需调整 FDT 保留策略。
        memory.reserved.push(self.storage);
        Ok(memory)
    }
}

#[cfg(feature = "ktest")]
#[path = "../ktest/boot/common/mod.rs"]
pub(crate) mod tests;
