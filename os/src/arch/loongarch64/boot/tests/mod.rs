//! 合成 QEMU direct boot 表的边界检查，不访问真实物理地址。

use alloc::{vec, vec::Vec};

use super::{mapped_address, tables, DIRECT_MAP_BASE, PHYSICAL_LIMIT};
use crate::mm::regions::Region;

const SYSTEM: usize = 0x200;
const CONFIG: usize = 0x400;
const INITRD: usize = 0x600;
const FDT_GUID: [u8; 16] = [
    0xd5, 0x21, 0xb6, 0xb1, 0x9c, 0xf1, 0xa5, 0x41, 0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0,
];
const INITRD_GUID: [u8; 16] = [
    0x27, 0xe4, 0x68, 0x55, 0xfc, 0x68, 0x3d, 0x4f, 0xac, 0x74, 0xca, 0x55, 0x52, 0x31, 0xcc, 0x68,
];

fn put(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Vec<u8> {
    let mut data = vec![0; 2048];
    put(&mut data, SYSTEM, 0x5453_5953_2049_4249);
    put(&mut data, SYSTEM + 104, 3);
    put(&mut data, SYSTEM + 112, CONFIG as u64);
    // 先放一个不相关的配置项，FDT 和 initrd 的发现不能依赖表项索引。
    data[CONFIG..CONFIG + 16].fill(0xa5);
    data[CONFIG + 24..CONFIG + 40].copy_from_slice(&INITRD_GUID);
    put(&mut data, CONFIG + 40, INITRD as u64);
    data[CONFIG + 48..CONFIG + 64].copy_from_slice(&FDT_GUID);
    put(&mut data, CONFIG + 64, 0x100000);
    data
}

fn valid_tables() {
    let mut data = fixture();
    let result = tables::parse(&data, SYSTEM).unwrap();
    assert_eq!(result.fdt, 0x100000);
    assert_eq!(result.initrd, None);
    put(&mut data, INITRD, 0x91000000);
    put(&mut data, INITRD + 8, 0x1234);
    let result = tables::parse(&data, SYSTEM).unwrap();
    assert_eq!(
        result.initrd,
        Some(Region::new(0x91000000, 0x1234).unwrap())
    );
    let (left, right) = data[CONFIG + 24..CONFIG + 72].split_at_mut(24);
    left.swap_with_slice(right);
    assert_eq!(tables::parse(&data, SYSTEM).unwrap().fdt, 0x100000);
}

fn invalid_tables() {
    for (offset, value) in [
        (SYSTEM, 0),
        (SYSTEM + 104, u64::MAX),
        (SYSTEM + 112, 2048),
        (SYSTEM + 112, (CONFIG + 1) as u64),
        (CONFIG + 40, 2040),
        (CONFIG + 64, 0),
        (CONFIG + 64, 0x100001),
        (INITRD, 1),     // 非零起点配零长度。
        (INITRD + 8, 1), // 零起点配非零长度。
    ] {
        let mut data = fixture();
        put(&mut data, offset, value);
        assert!(tables::parse(&data, SYSTEM).is_err());
    }
    let mut data = fixture();
    put(&mut data, INITRD, u64::MAX - 1);
    put(&mut data, INITRD + 8, 4096);
    assert!(tables::parse(&data, SYSTEM).is_err());
    assert!(tables::parse(&fixture(), SYSTEM + 1).is_err());
    assert!(tables::parse(&fixture(), usize::MAX - 7).is_err());
    assert!(tables::parse(&fixture()[..SYSTEM + 119], SYSTEM).is_err());

    for guid in [FDT_GUID, INITRD_GUID] {
        let mut data = fixture();
        data[CONFIG..CONFIG + 16].copy_from_slice(&guid);
        assert!(tables::parse(&data, SYSTEM).is_err());
    }
    for offset in [CONFIG + 24, CONFIG + 48] {
        let mut data = fixture();
        data[offset..offset + 16].fill(0);
        assert!(tables::parse(&data, SYSTEM).is_err());
    }
}

fn direct_mapping() {
    assert_eq!(
        mapped_address(0x90000000, 4096),
        Ok(DIRECT_MAP_BASE | 0x90000000)
    );
    assert!(mapped_address(PHYSICAL_LIMIT - 4096, 4096).is_ok());
    assert!(mapped_address(PHYSICAL_LIMIT - 4096, 4097).is_err());
    assert!(mapped_address(usize::MAX, 4096).is_err());
}

/// 验证 GUID 查找、initrd、截断/溢出/重复表项，以及 DMW 地址检查。
pub fn kernel_test() {
    valid_tables();
    invalid_tables();
    direct_mapping();
}
