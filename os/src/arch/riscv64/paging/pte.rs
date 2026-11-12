//! Sv39 表项编码；保留位、全局位和软件位均保持为零。

use super::Permissions;

const VALID: usize = 1;
const ACCESSED: usize = 1 << 6;
const DIRTY: usize = 1 << 7;
const PPN_MASK: usize = (1 << 44) - 1;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub(super) struct Entry(usize);

impl Entry {
    pub const EMPTY: Self = Self(0);

    pub fn branch(physical_address: usize) -> Self {
        Self(((physical_address >> 12) << 10) | VALID)
    }

    pub fn leaf(physical_address: usize, permissions: Permissions) -> Self {
        // 初版不追踪访问和脏状态，不依赖硬件自动更新 A/D，也不处理其软件故障。
        Self(Self::branch(physical_address).0 | permissions.0 | ACCESSED | DIRTY)
    }

    pub fn is_valid(self) -> bool {
        self.0 & VALID != 0
    }

    pub fn is_leaf(self) -> bool {
        self.0 & (Permissions::READ.0 | Permissions::EXECUTE.0) != 0
    }

    pub fn physical_address(self) -> usize {
        ((self.0 >> 10) & PPN_MASK) << 12
    }

    pub fn permissions(self) -> Permissions {
        Permissions(self.0 & Permissions::MASK)
    }
}

pub(super) fn indices(address: usize) -> Result<[usize; 3], super::Error> {
    // Sv39 的高 25 位必须是 bit 38 的符号扩展，不能只截取低 39 位。
    if ((address << 25) as isize >> 25) as usize != address {
        return Err(super::Error::InvalidVirtualAddress);
    }
    Ok([30, 21, 12].map(|shift| (address >> shift) & 0x1ff))
}

pub(super) fn valid_physical_address(address: usize) -> bool {
    address >> 56 == 0
}
