//! 解析 QEMU direct boot 的合成 EFI 配置表，不调用 EFI services。

use crate::mm::regions::Region;

const SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5953_2049_4249;
const SYSTEM_TABLE_SIZE: usize = 120;
const NUMBER_OF_TABLE_ENTRIES_OFFSET: usize = 104;
const CONFIGURATION_TABLE_OFFSET: usize = 112;
const CONFIG_ENTRY_SIZE: usize = 24;
const DEVICE_TREE_GUID: [u8; 16] = [
    0xd5, 0x21, 0xb6, 0xb1, 0x9c, 0xf1, 0xa5, 0x41, 0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0,
];
const INITRD_GUID: [u8; 16] = [
    0x27, 0xe4, 0x68, 0x55, 0xfc, 0x68, 0x3d, 0x4f, 0xac, 0x74, 0xca, 0x55, 0x52, 0x31, 0xcc, 0x68,
];

/// 从配置表按 GUID 找到的启动载荷。
pub(super) struct BootTables {
    pub fdt: usize,
    pub initrd: Option<Region>,
}

/// 解析 QEMU direct boot 的系统表、配置表数组和 initrd 描述。
///
/// 这些结构必须位于 `data` 表示的 QEMU 前 1 MiB 启动块内。FDT 载荷可以
/// 在启动块外，这里只检查其地址非零且按八字节对齐，由调用者映射并验证完整载荷。
pub(super) fn parse(data: &[u8], system_table: usize) -> Result<BootTables, &'static str> {
    let table = table_slice(data, system_table, SYSTEM_TABLE_SIZE)?;
    if read_u64_le(table, 0)? != SYSTEM_TABLE_SIGNATURE {
        return Err("invalid boot system table signature");
    }

    let count = read_usize_le(table, NUMBER_OF_TABLE_ENTRIES_OFFSET)?;
    let config = read_usize_le(table, CONFIGURATION_TABLE_OFFSET)?;
    let size = count
        .checked_mul(CONFIG_ENTRY_SIZE)
        .ok_or("boot table length overflow")?;
    let entries = table_slice(data, config, size)?;

    let mut fdt = None;
    let mut initrd_table = None;
    for entry in entries.chunks_exact(CONFIG_ENTRY_SIZE) {
        let guid = &entry[..16];
        if guid == DEVICE_TREE_GUID {
            if fdt.replace(read_usize_le(entry, 16)?).is_some() {
                return Err("duplicate boot FDT table");
            }
        } else if guid == INITRD_GUID && initrd_table.replace(read_usize_le(entry, 16)?).is_some() {
            return Err("duplicate boot initrd table");
        }
    }

    let fdt = fdt.ok_or("missing boot FDT table")?;
    if fdt == 0 || fdt % 8 != 0 {
        return Err("invalid boot FDT address");
    }

    // 当前 direct boot 即使没有 initrd 也提供零值描述。必须找到该描述，
    // 避免将缺失信息误解释为可覆盖的空闲 RAM。
    let initrd = table_slice(data, initrd_table.ok_or("missing boot initrd table")?, 16)?;
    let start = read_usize_le(initrd, 0)?;
    let size = read_usize_le(initrd, 8)?;
    let initrd = match (start, size) {
        (0, 0) => None,
        (_, 0) | (0, _) => return Err("invalid boot initrd range"),
        _ => Some(Region::new(start, size)?),
    };

    Ok(BootTables { fdt, initrd })
}

fn table_slice(data: &[u8], start: usize, size: usize) -> Result<&[u8], &'static str> {
    if start % 8 != 0 {
        return Err("unaligned boot table");
    }
    let end = start
        .checked_add(size)
        .ok_or("boot table address overflow")?;
    data.get(start..end)
        .ok_or("boot table outside startup block")
}

fn read_u64_le(data: &[u8], offset: usize) -> Result<u64, &'static str> {
    let end = offset.checked_add(8).ok_or("boot table offset overflow")?;
    let bytes = data.get(offset..end).ok_or("truncated boot table word")?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn read_usize_le(data: &[u8], offset: usize) -> Result<usize, &'static str> {
    usize::try_from(read_u64_le(data, offset)?).map_err(|_| "boot value exceeds usize")
}
