//! 通过真实用户态执行验证 ELF 映射、启动栈和装载失败后的页框回收。

use core::{arch::global_asm, ptr, slice};

use super::{elf, Error, LoadedProgram};
use crate::{
    arch,
    ktest::support::{elf as elf_fixture, frames::with_small_pool},
    mm::{
        self,
        address_space::AddressSpace,
        frame::{FrameAllocator, PAGE_SIZE},
    },
    task::{user_stack, ExitStatus},
};

const PH_COUNT: u16 = 2;
const CODE_OFFSET: usize = 0x100;
const TEXT_PAGE: usize = 0x10000;
const TEXT_FLAGS: u32 = elf_fixture::PF_R | elf_fixture::PF_X;
const DATA_FLAGS: u32 = elf_fixture::PF_R | elf_fixture::PF_W;
const PHDR_ADDRESS: usize = TEXT_PAGE + elf_fixture::ph_offset(0);

// 数据与代码相距超过 2 MiB，后续段在 Sv39 中需要独立的中间表分支。
const SV39_LEAF_TABLE_SPAN: usize = 512 * PAGE_SIZE;
const DATA_PAGE: usize = TEXT_PAGE + SV39_LEAF_TABLE_SPAN + PAGE_SIZE;
const DATA_OFFSET: usize = 0xf81;
const DATA_ADDRESS: usize = DATA_PAGE + DATA_OFFSET;
const DATA_FILE_OFFSET: usize = PAGE_SIZE + DATA_OFFSET;
const DATA_FILE_SIZE: usize = 0x192;
const DATA_FILE_END: usize = DATA_ADDRESS + DATA_FILE_SIZE;
const DATA_MEMORY_END: usize = DATA_PAGE + 2 * PAGE_SIZE + 0x22b;
const DATA_PAGES_END: usize = DATA_PAGE + 3 * PAGE_SIZE;
const IMAGE_SIZE: usize = DATA_FILE_OFFSET + DATA_FILE_SIZE;
const PATTERN_MASK: usize = 0x7f;

const ARGV0: &[u8] = b"prog";
const ENVP0: &[u8] = b"K=V";
const ENVP_ADDRESS: usize = AddressSpace::STACK_TOP - (ENVP0.len() + 1);
const ARGV_ADDRESS: usize = ENVP_ADDRESS - (ARGV0.len() + 1);
const _: () = {
    assert!(elf_fixture::ph_offset(PH_COUNT as usize) <= CODE_OFFSET);
    assert!(ARGV_ADDRESS & 1 == 1);
};
const POOL_CAPACITY: usize = 32;

global_asm!(
    include_str!("loader/riscv64.S"),
    argv_address = const ARGV_ADDRESS,
    envp_address = const ENVP_ADDRESS,
    at_entry = const user_stack::AT_ENTRY,
    at_pagesz = const user_stack::AT_PAGESZ,
    at_phdr = const user_stack::AT_PHDR,
    at_phent = const user_stack::AT_PHENT,
    at_phnum = const user_stack::AT_PHNUM,
    page_size = const PAGE_SIZE,
    phdr_address = const PHDR_ADDRESS,
    phent_size = const elf_fixture::ELF64_PHENT_SIZE,
    phnum = const PH_COUNT,
    pt_load = const elf_fixture::PT_LOAD,
    ph_flags = const elf_fixture::PH_FLAGS,
    text_flags = const TEXT_FLAGS,
    data_page = const DATA_PAGE,
    data_address = const DATA_ADDRESS,
    data_file_end = const DATA_FILE_END,
    data_memory_end = const DATA_MEMORY_END,
    data_pages_end = const DATA_PAGES_END,
    pattern_mask = const PATTERN_MASK,
);

extern "C" {
    static loader_probe_with_phdr: u8;
    static loader_probe_without_phdr: u8;
    static loader_probe_end: u8;
}

#[derive(Clone, Copy, Debug)]
enum PhdrLayout {
    Readable,
    ExecuteOnly,
    Uncovered,
}

fn write_image(image: &mut [u8; IMAGE_SIZE], layout: PhdrLayout) {
    image.fill(0);
    let start = ptr::addr_of!(loader_probe_with_phdr);
    let size = ptr::addr_of!(loader_probe_end) as usize - start as usize;
    let without_phdr = ptr::addr_of!(loader_probe_without_phdr) as usize - start as usize;
    assert!(without_phdr < size && CODE_OFFSET + size <= PAGE_SIZE);
    // SAFETY: 汇编符号标记同一只读探针的连续字节范围，长度不含末尾符号。
    let code = unsafe { slice::from_raw_parts(start, size) };
    elf_fixture::put(image, CODE_OFFSET, code);
    for (i, byte) in image[DATA_FILE_OFFSET..].iter_mut().enumerate() {
        *byte = ((i & PATTERN_MASK) + 1) as u8;
    }

    let (flags, offset, entry_offset) = match layout {
        PhdrLayout::Readable => (TEXT_FLAGS, 0, 0),
        PhdrLayout::ExecuteOnly => (elf_fixture::PF_X, 0, without_phdr),
        PhdrLayout::Uncovered => (TEXT_FLAGS, CODE_OFFSET, without_phdr),
    };
    let entry = TEXT_PAGE + CODE_OFFSET + entry_offset;
    let text_size = (CODE_OFFSET + size - offset) as u64;
    elf_fixture::put_header(image, arch::ELF_MACHINE, entry as u64, PH_COUNT);
    elf_fixture::put_load(
        image,
        elf_fixture::ph_offset(0),
        elf_fixture::LoadSpec {
            flags,
            file_offset: offset as u64,
            virtual_address: (TEXT_PAGE + offset) as u64,
            file_size: text_size,
            memory_size: text_size,
        },
    );
    elf_fixture::put_load(
        image,
        elf_fixture::ph_offset(1),
        elf_fixture::LoadSpec {
            flags: DATA_FLAGS,
            file_offset: DATA_FILE_OFFSET as u64,
            virtual_address: DATA_ADDRESS as u64,
            file_size: DATA_FILE_SIZE as u64,
            memory_size: (DATA_MEMORY_END - DATA_ADDRESS) as u64,
        },
    );
}

fn load<'a>(pool: &'a FrameAllocator, image: &[u8]) -> Result<LoadedProgram<'a>, Error> {
    // SAFETY: 自检在关中断的 S-mode 启动主核上执行；调用方只传入共享夹具从真实 RAM 借出的独占页框池。
    unsafe { LoadedProgram::load_in(pool, image, &[ARGV0], &[ENVP0]) }
}

fn check_execution(frames: &FrameAllocator, image: &[u8], layout: PhdrLayout) -> usize {
    let mut needed = 0;
    with_small_pool(frames, POOL_CAPACITY, |pool| {
        assert!(arch::user_paging::is_user_inactive());
        let reserved: [_; POOL_CAPACITY] = core::array::from_fn(|_| pool.alloc().unwrap());
        for frame in &reserved {
            // SAFETY: Bare 下可按物理地址访问夹具借出的真实 RAM；每个句柄独占整个页框。
            unsafe { ptr::write_bytes(frame.start_address() as *mut u8, 0xa5, PAGE_SIZE) };
        }
        drop(reserved);

        let mut program = load(pool, image).unwrap();
        needed = POOL_CAPACITY - pool.available();
        // SAFETY: Trap 已安装，支持 Sv39 的启动主核在 S-mode 下关闭中断。
        // 程序对象与 Trap 现场在同址映射的启动栈中；探针只访问装载器建立的用户映射并同步退出。
        let status = unsafe { program.run() };
        assert_eq!(status, Ok(ExitStatus::Exited(0)), "{layout:?}");
        assert!(arch::user_paging::is_user_inactive());
        drop(program);
        assert_eq!(pool.available(), POOL_CAPACITY, "{layout:?}");
    });
    needed
}

fn check_oom(frames: &FrameAllocator, image: &[u8], needed: usize) {
    for capacity in 0..needed {
        with_small_pool(frames, capacity, |pool| {
            assert_eq!(
                load(pool, image).err(),
                Some(Error::OutOfMemory),
                "页框池容量 {capacity}"
            );
        });
    }
    with_small_pool(frames, needed, |pool| {
        let program = load(pool, image).unwrap();
        assert_eq!(pool.available(), 0);
        drop(program);
        assert_eq!(pool.available(), needed);
    });
}

fn check_active_rejection(frames: &FrameAllocator, image: &[u8]) {
    let before = frames.available();
    let program = load(frames, image).unwrap();
    let active = frames.available();
    // SAFETY: 自检在关中断的 S-mode 启动主核上执行；栈、镜像和分配器元数据保持同址可访问。
    // 再次装载只解析内核镜像，构建根页表时即拒绝活动态，不访问未映射的页框。
    unsafe {
        program
            .space
            .with_active(|| {
                assert_eq!(load(frames, image).err(), Some(Error::PagingUnavailable));
                assert_eq!(frames.available(), active);
                assert!(!arch::user_paging::is_user_inactive());
            })
            .unwrap();
    }
    assert!(arch::user_paging::is_user_inactive());
    drop(program);
    assert_eq!(frames.available(), before);
}

fn check_load_rejections(frames: &FrameAllocator, image: &mut [u8]) {
    static OVERSIZED_ARG: [u8; PAGE_SIZE] = [b'x'; PAGE_SIZE];
    let load_err = |image: &[u8], argv: &[&[u8]]| {
        // SAFETY: 当前为关中断的 S-mode 启动主核，全局分配器已初始化并管理独占的真实 RAM。
        unsafe { LoadedProgram::load(image, argv, &[]) }.err()
    };
    let before = frames.available();
    assert_eq!(
        load_err(&[], &[]),
        Some(Error::InvalidElf(elf::Error::TruncatedHeader))
    );
    assert_eq!(frames.available(), before);

    assert_eq!(
        load_err(image, &[&OVERSIZED_ARG]),
        Some(Error::StackTooLarge)
    );
    assert_eq!(frames.available(), before);

    assert_eq!(
        load_err(image, &[b"invalid\0arg"]),
        Some(Error::InvalidStackArguments)
    );
    assert_eq!(frames.available(), before);

    let data_header = elf_fixture::ph_offset(1);
    elf_fixture::put_u32(
        image,
        data_header + elf_fixture::PH_FLAGS,
        DATA_FLAGS | elf_fixture::PF_X,
    );
    assert_eq!(load_err(image, &[]), Some(Error::UnsupportedPermissions));
    assert_eq!(frames.available(), before);

    elf_fixture::put_u32(image, data_header + elf_fixture::PH_FLAGS, DATA_FLAGS);
    elf_fixture::put_u64(
        image,
        data_header + elf_fixture::PH_VADDR,
        (AddressSpace::STACK_TOP - PAGE_SIZE + DATA_OFFSET) as u64,
    );
    assert_eq!(load_err(image, &[]), Some(Error::InvalidSegmentAddress));
    assert_eq!(frames.available(), before);
    assert!(arch::user_paging::is_user_inactive());
}

/// 验证静态 ELF 的用户态可见内容、程序头辅助向量与失败回收。
pub(crate) fn kernel_test() {
    let frames = mm::frames();
    let mut image = [0; IMAGE_SIZE];
    write_image(&mut image, PhdrLayout::Readable);
    let needed = check_execution(frames, &image, PhdrLayout::Readable);
    check_oom(frames, &image, needed);
    check_active_rejection(frames, &image);
    check_load_rejections(frames, &mut image);

    for layout in [PhdrLayout::ExecuteOnly, PhdrLayout::Uncovered] {
        write_image(&mut image, layout);
        check_execution(frames, &image, layout);
    }
}
