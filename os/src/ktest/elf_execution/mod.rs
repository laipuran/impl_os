//! 通过真实 U-mode 验证 ELF 映射、启动栈以及失败回收。

use core::{arch::global_asm, ptr, slice};

use super::{Error, LoadedProgram};
use crate::{
    arch::{self, user_paging::is_user_inactive},
    mm::{self, address_space::AddressSpace, frame::PAGE_SIZE},
    task::{user_stack, ExitStatus},
};

global_asm!(include_str!("probe.S"));

extern "C" {
    static elf_execution_start: u8;
    static elf_execution_end: u8;
}

const TEXT_HEADER: usize = 64;
const DATA_HEADER: usize = 120;
const CODE_OFFSET: usize = 0x100;
const DATA_OFFSET: usize = 0x380;

fn put(image: &mut [u8], offset: usize, bytes: impl AsRef<[u8]>) {
    let bytes = bytes.as_ref();
    image[offset..offset + bytes.len()].copy_from_slice(bytes);
}

fn image() -> [u8; 0x400] {
    let mut image = [0; 0x400];
    put(&mut image, 0, b"\x7fELF\x02\x01\x01");
    put(&mut image, 16, 2u16.to_le_bytes()); // ET_EXEC
    put(&mut image, 18, arch::ELF_MACHINE.to_le_bytes());
    put(&mut image, 20, 1u32.to_le_bytes());
    put(&mut image, 24, 0x10100u64.to_le_bytes());
    put(&mut image, 32, (TEXT_HEADER as u64).to_le_bytes());
    put(&mut image, 52, 64u16.to_le_bytes());
    put(&mut image, 54, 56u16.to_le_bytes());
    put(&mut image, 56, 2u16.to_le_bytes());

    let start = ptr::addr_of!(elf_execution_start);
    let size = ptr::addr_of!(elf_execution_end) as usize - start as usize;
    assert!(size > 0 && CODE_OFFSET + size < DATA_OFFSET);
    // SAFETY: 汇编文件中的两个符号标记同一只读探针的连续字节范围。
    let code = unsafe { slice::from_raw_parts(start, size) };
    put(&mut image, CODE_OFFSET, code);
    put(&mut image, DATA_OFFSET, [42]);
    for (header, flags, fields) in [
        (
            TEXT_HEADER,
            5u32,
            [
                0,
                0x10000,
                0,
                (CODE_OFFSET + size) as u64,
                (CODE_OFFSET + size) as u64,
                0x1000,
            ],
        ),
        (
            DATA_HEADER,
            6u32,
            [DATA_OFFSET as u64, 0x12380, 0, 1, 0xf00, 0x1000],
        ),
    ] {
        put(&mut image, header, 1u32.to_le_bytes()); // PT_LOAD
        put(&mut image, header + 4, flags.to_le_bytes());
        for (index, value) in fields.into_iter().enumerate() {
            put(&mut image, header + 8 + index * 8, value.to_le_bytes());
        }
    }
    image
}

fn load(image: &[u8], argv: &[&[u8]], envp: &[&[u8]]) -> Result<LoadedProgram, Error> {
    // SAFETY: 自检在关中断的启动主核运行，页框分配器已初始化并管理独占 RAM。
    unsafe { LoadedProgram::load(image, argv, envp) }
}

pub(crate) fn kernel_test() {
    let image = image();
    let before = mm::frames().available();
    {
        let mut program = load(&image, &[b"prog"], &[b"K=V"]).unwrap();
        // SAFETY: 自检在已安装 Trap 的启动主核上关中断执行，程序位于启动栈，
        // 探针只验证自身映射且通过 exit 返回。
        assert_eq!(unsafe { program.run() }, Ok(ExitStatus::Exited(42)));
        assert!(is_user_inactive());
    }
    assert_eq!(mm::frames().available(), before);

    let mut bad = image;
    put(&mut bad, DATA_HEADER + 4, 7u32.to_le_bytes());
    assert!(matches!(
        load(&bad, &[], &[]),
        Err(Error::UnsupportedPermissions)
    ));
    assert_eq!(mm::frames().available(), before);

    bad = image;
    put(
        &mut bad,
        DATA_HEADER + 16,
        ((AddressSpace::STACK_TOP - PAGE_SIZE) as u64 + DATA_OFFSET as u64).to_le_bytes(),
    );
    assert!(matches!(
        load(&bad, &[], &[]),
        Err(Error::Memory(mm::UserMappingError::InvalidUserAddress))
    ));
    assert_eq!(mm::frames().available(), before);
    assert!(matches!(
        load(&image, &[&[b'x'; PAGE_SIZE]], &[]),
        Err(Error::Stack(user_stack::Error::InsufficientSpace))
    ));
    assert_eq!(mm::frames().available(), before);
    assert!(is_user_inactive());
}
