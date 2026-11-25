//! 通过手工编码的最小 ELF64 镜像，白盒检验不带程序解释器的 `ET_EXEC` 镜像解析与装载契约。

use super::{parse, Error};
use crate::ktest::support::elf as elf_fixture;

const IMAGE_SIZE: usize = 0x200;
const TEXT_PH: usize = elf_fixture::ph_offset(0);
const DATA_PH: usize = elf_fixture::ph_offset(1);

fn image_for(machine: u16) -> [u8; IMAGE_SIZE] {
    let mut image = [0; IMAGE_SIZE];
    elf_fixture::put_header(&mut image, machine, 0x100b0, 2);

    // 表位于首个 LOAD 内，但不在段起点；数据段的 BSS 跨过页边界。
    elf_fixture::put_load(
        &mut image,
        TEXT_PH,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R | elf_fixture::PF_X,
            file_offset: 0x20,
            virtual_address: 0x10020,
            file_size: 0xa0,
            memory_size: 0xa0,
        },
    );
    elf_fixture::put_load(
        &mut image,
        DATA_PH,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R | elf_fixture::PF_W,
            file_offset: 0x180,
            virtual_address: 0x12180,
            file_size: 4,
            memory_size: 0xf00,
        },
    );
    image[0xb0] = 0xaa;
    elf_fixture::put(&mut image, 0x180, &[0xde, 0xad, 0xbe, 0xef]);
    image
}

fn check_load_plan(image: &[u8; IMAGE_SIZE]) {
    let elf = parse(image).unwrap();
    assert_eq!(elf.entry(), 0x100b0);
    let headers = elf.program_headers();
    assert_eq!(headers.file_offset, elf_fixture::ELF64_HEADER_SIZE);
    assert_eq!(headers.entry_size, elf_fixture::ELF64_PHENT_SIZE);
    assert_eq!(headers.count, 2);
    assert_eq!(headers.virtual_address, Some(0x10040));

    let mut loads = elf.load_segments();
    let text = loads.next().unwrap();
    assert_eq!(text.memory, 0x10020..0x100c0);
    assert_eq!(text.pages, 0x10000..0x11000);
    assert_eq!(text.data, &image[0x20..0xc0]);
    assert_eq!(text.data_offset, 0x20);
    assert_eq!(text.zero_fill, 0x100c0..0x100c0);
    assert!(text.permissions.read && !text.permissions.write && text.permissions.execute);

    let data = loads.next().unwrap();
    assert_eq!(data.memory, 0x12180..0x13080);
    assert_eq!(data.pages, 0x12000..0x14000);
    assert_eq!(data.data, &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(data.data_offset, 0x180);
    assert_eq!(data.zero_fill, 0x12184..0x13080);
    assert!(data.permissions.read && data.permissions.write && !data.permissions.execute);
    assert!(loads.next().is_none());
}

fn check_optional_headers(image: &[u8; IMAGE_SIZE]) {
    // 缩短首段文件大小，使文件映射上界（0xaf）比程序头表上界（0xb0）少 1 字节，验证 ProgramHeaders::virtual_address 回退为 None。
    let mut partial = *image;
    elf_fixture::put_u64(&mut partial, TEXT_PH + elf_fixture::PH_FILESZ, 0x8f);
    elf_fixture::put_u64(&mut partial, elf_fixture::EH_ENTRY, 0x100a0);
    elf_fixture::put_u32(
        &mut partial,
        DATA_PH + elf_fixture::PH_TYPE,
        elf_fixture::PT_NULL,
    );
    let elf = parse(&partial).unwrap();
    assert_eq!(elf.entry(), 0x100a0);
    assert_eq!(elf.program_headers().virtual_address, None);
    assert_eq!(elf.load_segments().count(), 1);
}

fn check_declared_phdr(image: &[u8; IMAGE_SIZE]) {
    // 构造两份均完整映射程序头表的加载段副本，验证 PT_PHDR 优先选中其声明的第二份地址。
    let mut copies = *image;
    let text = elf_fixture::ph_offset(2);
    let data = elf_fixture::ph_offset(3);
    let table_size = (4 * elf_fixture::ELF64_PHENT_SIZE) as u64;
    elf_fixture::put_u16(&mut copies, elf_fixture::EH_PHNUM, 4);
    elf_fixture::put_u64(&mut copies, elf_fixture::EH_ENTRY, 0x10120);
    elf_fixture::put_load(
        &mut copies,
        TEXT_PH,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R,
            file_offset: 0x40,
            virtual_address: 0x12040,
            file_size: table_size,
            memory_size: table_size,
        },
    );
    elf_fixture::put_u32(
        &mut copies,
        TEXT_PH + elf_fixture::PH_TYPE,
        elf_fixture::PT_PHDR,
    );
    elf_fixture::put_u64(&mut copies, TEXT_PH + elf_fixture::PH_ALIGN, 8);
    copies[DATA_PH..text].fill(0); // PT_NULL，为重复声明和顺序测试保留槽位。
    elf_fixture::put_load(
        &mut copies,
        text,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R | elf_fixture::PF_X,
            file_offset: 0,
            virtual_address: 0x10000,
            file_size: 0x130,
            memory_size: 0x130,
        },
    );
    elf_fixture::put_load(
        &mut copies,
        data,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R,
            file_offset: 0,
            virtual_address: 0x12000,
            file_size: 0x130,
            memory_size: 0x130,
        },
    );
    assert_eq!(
        parse(&copies).unwrap().program_headers().virtual_address,
        Some(0x12040)
    );

    // 两条声明完全相同，且都在首个 PT_LOAD 之前。
    let mut duplicate = copies;
    duplicate.copy_within(TEXT_PH..DATA_PH, DATA_PH);
    assert_eq!(parse(&duplicate).unwrap_err(), Error::InvalidPhdrMapping);

    // 将首个 PT_LOAD 移到声明之前，保持两个加载段的相对顺序。
    let mut late = copies;
    late.copy_within(text..data, DATA_PH);
    late.copy_within(TEXT_PH..DATA_PH, text);
    late[TEXT_PH..DATA_PH].fill(0);
    assert_eq!(parse(&late).unwrap_err(), Error::InvalidPhdrMapping);

    for (field, value) in [
        (elf_fixture::PH_OFFSET, 0x48),
        (elf_fixture::PH_FILESZ, table_size - 8),
        (elf_fixture::PH_MEMSZ, table_size - 8),
    ] {
        reject_change(
            &copies,
            TEXT_PH + field,
            &value.to_le_bytes(),
            Error::InvalidPhdrMapping,
        );
    }

    // 声明落在第二段内，但没有指向表的文件字节。
    reject_change(
        &copies,
        TEXT_PH + elf_fixture::PH_VADDR,
        &0x12048_u64.to_le_bytes(),
        Error::InvalidPhdrMapping,
    );
    // 表的最后一个字节在第二段中变成 BSS；不能退回第一份副本。
    reject_change(
        &copies,
        data + elf_fixture::PH_FILESZ,
        &0x11f_u64.to_le_bytes(),
        Error::InvalidPhdrMapping,
    );
}

fn check_alignment(image: &[u8; IMAGE_SIZE]) {
    let mut shifted = *image;
    shifted.copy_within(0x40..0xb0, 0x41);
    elf_fixture::put_u64(&mut shifted, elf_fixture::EH_PHOFF, 0x41);
    assert_eq!(parse(&shifted).unwrap_err(), Error::InvalidProgramHeaders);

    reject_change(
        image,
        elf_fixture::EH_ENTRY,
        &0x100b1_u64.to_le_bytes(),
        Error::InvalidEntry,
    );
    let mut halfword = *image;
    elf_fixture::put_u64(&mut halfword, elf_fixture::EH_ENTRY, 0x100b2);
    if crate::arch::NAME == "riscv64" {
        assert_eq!(parse(&halfword).unwrap().entry(), 0x100b2);
    } else {
        assert_eq!(parse(&halfword).unwrap_err(), Error::InvalidEntry);
    }
}

fn check_empty_load(image: &[u8; IMAGE_SIZE]) {
    let mut empty = *image;
    // 空段虽起于文本页内，但不占用该页；0 和 1 均不额外约束 p_align。
    elf_fixture::put_load(
        &mut empty,
        DATA_PH,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R | elf_fixture::PF_W,
            file_offset: 0x180,
            virtual_address: 0x10180,
            file_size: 0,
            memory_size: 0,
        },
    );
    for alignment in [0, 1] {
        elf_fixture::put_u64(&mut empty, DATA_PH + elf_fixture::PH_ALIGN, alignment);
        assert_eq!(parse(&empty).unwrap().load_segments().count(), 1);
    }
    // 纯 BSS 不读取文件，偏移可以超过文件末尾。
    let mut bss = *image;
    elf_fixture::put_load(
        &mut bss,
        DATA_PH,
        elf_fixture::LoadSpec {
            flags: elf_fixture::PF_R | elf_fixture::PF_W,
            file_offset: 0x1200,
            virtual_address: 0x11200,
            file_size: 0,
            memory_size: 0xe00,
        },
    );
    let load = parse(&bss).unwrap().load_segments().nth(1).unwrap();
    assert!(load.data.is_empty());
    assert_eq!(load.pages, 0x11000..0x12000);
    assert_eq!(load.zero_fill, 0x11200..0x12000);

    // 即使 p_align 放宽，空段仍须满足页同余。
    reject_change(
        &empty,
        DATA_PH + elf_fixture::PH_VADDR,
        &0x10181_u64.to_le_bytes(),
        Error::InvalidSegmentAlignment,
    );
}

fn reject_change(image: &[u8; IMAGE_SIZE], offset: usize, bytes: &[u8], expected: Error) {
    let mut malformed = *image;
    elf_fixture::put(&mut malformed, offset, bytes);
    assert_eq!(
        parse(&malformed).unwrap_err(),
        expected,
        "字段偏移 {offset:#x}"
    );
}

fn check_invalid_headers(image: &[u8; IMAGE_SIZE]) {
    assert_eq!(
        parse(&image[..elf_fixture::ELF64_HEADER_SIZE - 1]).unwrap_err(),
        Error::TruncatedHeader
    );
    for (offset, value, expected) in [
        (elf_fixture::EH_MAGIC, 0, Error::BadMagic),
        (elf_fixture::EH_CLASS, 1, Error::UnsupportedClass),
        (elf_fixture::EH_DATA, 2, Error::UnsupportedEndian),
        (elf_fixture::EH_IDENT_VERSION, 2, Error::UnsupportedVersion),
    ] {
        reject_change(image, offset, &[value], expected);
    }
    for (offset, value, expected) in [
        (elf_fixture::EH_TYPE, 3_u16, Error::UnsupportedType), // ET_DYN
        (elf_fixture::EH_EHSIZE, 63, Error::InvalidHeaderSize),
        (elf_fixture::EH_PHENTSIZE, 55, Error::InvalidProgramHeaders),
        (elf_fixture::EH_PHNUM, 0, Error::InvalidProgramHeaders),
        (elf_fixture::EH_PHNUM, 0xffff, Error::InvalidProgramHeaders),
    ] {
        reject_change(image, offset, &value.to_le_bytes(), expected);
    }
    reject_change(
        image,
        elf_fixture::EH_VERSION,
        &2_u32.to_le_bytes(),
        Error::UnsupportedVersion,
    );
    for offset in [0x38_u64, 0x198, u64::MAX - 63] {
        reject_change(
            image,
            elf_fixture::EH_PHOFF,
            &offset.to_le_bytes(),
            Error::InvalidProgramHeaders,
        );
    }
}

fn check_invalid_loads(image: &[u8; IMAGE_SIZE]) {
    reject_change(
        image,
        DATA_PH + elf_fixture::PH_TYPE,
        &elf_fixture::PT_INTERP.to_le_bytes(),
        Error::UnsupportedInterpreter,
    );
    for (offset, value, expected) in [
        (elf_fixture::PH_MEMSZ, 3_u64, Error::FileLargerThanMemory),
        (elf_fixture::PH_ALIGN, 3, Error::InvalidSegmentAlignment),
        // 页同余成立，p_align 同余失败。
        (
            elf_fixture::PH_ALIGN,
            0x4000,
            Error::InvalidSegmentAlignment,
        ),
        (elf_fixture::PH_FILESZ, 0x81, Error::SegmentFileOutOfBounds),
        (elf_fixture::PH_VADDR, 0xe180, Error::UnsortedLoadSegments),
        // 字节范围不重叠，页范围重叠。
        (elf_fixture::PH_VADDR, 0x10180, Error::OverlappingLoadPages),
    ] {
        reject_change(image, DATA_PH + offset, &value.to_le_bytes(), expected);
    }
    for entry in [0x100c0_u64, 0x12180] {
        reject_change(
            image,
            elf_fixture::EH_ENTRY,
            &entry.to_le_bytes(),
            Error::InvalidEntry,
        );
    }

    let high_address = u64::MAX - 0xe7f; // 保留 0x180 的页内偏移。
    let mut malformed = *image;
    elf_fixture::put_u64(&mut malformed, DATA_PH + elf_fixture::PH_FILESZ, 0xf00);
    reject_change(
        &malformed,
        DATA_PH + elf_fixture::PH_OFFSET,
        &high_address.to_le_bytes(),
        Error::SegmentFileOutOfBounds,
    );
    reject_change(
        image,
        DATA_PH + elf_fixture::PH_VADDR,
        &high_address.to_le_bytes(),
        Error::SegmentAddressOverflow,
    );
    // 内存末址恰为 u64::MAX，只有将其向上取整到页边界才会溢出。
    malformed = *image;
    elf_fixture::put_u64(&mut malformed, DATA_PH + elf_fixture::PH_MEMSZ, 0xe7f);
    reject_change(
        &malformed,
        DATA_PH + elf_fixture::PH_VADDR,
        &high_address.to_le_bytes(),
        Error::SegmentAddressOverflow,
    );
}

/// 检验 ELF64 元数据、段装载计划以及畸形镜像的诊断。
pub(crate) fn kernel_test() {
    let (native, foreign) = match crate::arch::NAME {
        "riscv64" => (243, 258),
        "loongarch64" => (258, 243),
        name => panic!("没有为 {name} 定义 ELF 机器号"),
    };
    let image = image_for(native);
    check_load_plan(&image);
    assert_eq!(parse(&image_for(foreign)).unwrap_err(), Error::WrongMachine);
    check_optional_headers(&image);
    check_declared_phdr(&image);
    check_alignment(&image);
    check_empty_load(&image);
    check_invalid_headers(&image);
    check_invalid_loads(&image);
}
