//! 通过地址空间的构造和执行接口验证真实用户权限、隔离及页框回收。

use core::{arch::global_asm, ptr, slice};

use super::{AddressSpace, STACK_GUARD, STACK_PAGE};
use crate::arch::{
    paging::{Error as PagingError, Permissions},
    syscall::ARGUMENT_START,
    trap::{
        cause::{ExceptionCause, TrapCause},
        TrapContext,
    },
    user_paging::{init_zeroed_frame, is_user_inactive},
    UART_BASE,
};
use crate::ktest::support::frames::with_small_pool;
use crate::mm::{
    frame::{FrameAllocator, PAGE_SIZE},
    Access, UserMappingError,
};
use crate::task::{ExitStatus, Task};

global_asm!(include_str!("riscv64.S"));

extern "C" {
    static riscv_user_space_start: u8;
    static riscv_user_space_load: u8;
    static riscv_user_space_read_byte: u8;
    static riscv_user_space_store: u8;
    static riscv_user_space_code_byte: u8;
    static riscv_user_space_end: u8;
    static skernel: u8;
    static srodata: u8;
    static sdata: u8;
}

const USER_CODE_ADDRESS: usize = 0x1_0000;
const USER_DATA_ADDRESS: usize = 0x2_0000;
const SINGLE_SPACE_ADDRESS: usize = 0x3_0000;
const EXECUTE_ONLY_ADDRESS: usize = 0x4_0000;

fn probe_code() -> &'static [u8] {
    let start = ptr::addr_of!(riscv_user_space_start);
    let len = ptr::addr_of!(riscv_user_space_end) as usize - start as usize;
    assert!((1..=PAGE_SIZE).contains(&len));
    // SAFETY: 首尾符号标记同一段连续、已初始化的静态只读字节。
    unsafe { slice::from_raw_parts(start, len) }
}

fn with_probes(mut space: AddressSpace<'_>, initial: u8) -> AddressSpace<'_> {
    space
        .map_page(USER_CODE_ADDRESS, Access::ReadExecute, 0, probe_code())
        .unwrap();
    space
        .map_page(USER_DATA_ADDRESS, Access::ReadWrite, 0, &[initial])
        .unwrap();
    space
}

fn new_space(initial: u8) -> AddressSpace<'static> {
    // SAFETY: 自检在关中断的 S-mode 启动主核上执行，已初始化的全局分配器管理独占真实 RAM。
    let space = unsafe { AddressSpace::new() }.unwrap();
    with_probes(space, initial)
}

fn check_layout(space: &mut AddressSpace<'_>) {
    let text = ptr::addr_of!(skernel) as usize;
    let rodata = ptr::addr_of!(srodata) as usize;
    let data = ptr::addr_of!(sdata) as usize;
    for (address, permissions) in [
        (text, Permissions::READ | Permissions::EXECUTE),
        (rodata, Permissions::READ),
        (data, Permissions::READ | Permissions::WRITE),
        (UART_BASE, Permissions::READ | Permissions::WRITE),
    ] {
        let entry = space.table.translate(address).unwrap();
        assert_eq!(entry.physical_address, address);
        assert_eq!(entry.permissions, permissions);
    }
    for (address, permissions) in [
        (
            USER_CODE_ADDRESS,
            Permissions::READ | Permissions::EXECUTE | Permissions::USER,
        ),
        (
            USER_DATA_ADDRESS,
            Permissions::READ | Permissions::WRITE | Permissions::USER,
        ),
        (
            STACK_PAGE,
            Permissions::READ | Permissions::WRITE | Permissions::USER,
        ),
    ] {
        assert_eq!(
            space.table.translate(address).unwrap().permissions,
            permissions
        );
    }
    assert_eq!(
        space.table.translate(STACK_GUARD),
        Err(PagingError::NotMapped)
    );

    let before = crate::mm::frames().available();
    assert_eq!(
        space
            .table
            .ensure_unmapped(USER_DATA_ADDRESS + PAGE_SIZE + 1),
        Err(UserMappingError::InvalidMapping),
    );
    for address in [
        0,
        USER_CODE_ADDRESS + 1,
        STACK_GUARD,
        STACK_PAGE,
        UART_BASE,
        text,
    ] {
        assert_eq!(
            space.map_page(address, Access::ReadWrite, 0, &[]),
            Err(UserMappingError::InvalidUserAddress),
        );
    }
    assert_eq!(
        space.map_page(USER_DATA_ADDRESS, Access::ReadWrite, 0, &[99]),
        Err(UserMappingError::AlreadyMapped),
    );
    assert_eq!(
        space.map_page(
            USER_DATA_ADDRESS + PAGE_SIZE,
            Access::ReadWrite,
            0,
            &[0; PAGE_SIZE + 1],
        ),
        Err(UserMappingError::InitialDataTooLarge),
    );
    assert_eq!(crate::mm::frames().available(), before);
}

fn run_probe(
    space: &AddressSpace<'_>,
    entry: usize,
    probe_arg: usize,
    expected: ExitStatus,
) -> Task {
    let mut context = TrapContext::user(entry, AddressSpace::STACK_TOP);
    context.registers_mut()[ARGUMENT_START] = probe_arg;
    let mut task = Task::new(context);
    let before = crate::mm::frames().available();
    // SAFETY: 自检在支持 Sv39 且已安装 Trap 的 S-mode 启动主核上关中断执行；任务与 Trap 现场位于内核镜像启动栈。
    // 用户入口与栈指针均满足对齐要求且来自本地址空间映射，探针同步返回且任务循环不通过内核指针访问未映射表页。
    assert_eq!(unsafe { space.with_active(|| task.run()) }, Ok(expected));
    assert!(is_user_inactive());
    assert_eq!(crate::mm::frames().available(), before);
    task
}

fn check_fault(space: &AddressSpace<'_>, entry: usize, addr: usize, cause: ExceptionCause) {
    let task = run_probe(space, entry, addr, ExitStatus::Faulted);
    assert_eq!(task.context().cause(), TrapCause::Exception(cause));
    assert_eq!(task.context().pc(), entry);
    assert_eq!(task.context().bad_addr(), addr);
}

fn check_active_rejections(
    first: &AddressSpace<'_>,
    second: &mut AddressSpace<'_>,
    read_byte: usize,
) {
    use UserMappingError::UnavailableInCurrentMode;

    let frames = crate::mm::frames();
    let mut frame = frames.alloc().unwrap();
    let before = frames.available();
    let mut context = TrapContext::user(read_byte, AddressSpace::STACK_TOP);
    context.registers_mut()[ARGUMENT_START] = SINGLE_SPACE_ADDRESS;
    let mut task = Task::new(context);

    // SAFETY: 启动主核在支持 Sv39 的 S-mode 关中断运行，Trap 已安装。内核对象都在同地址映射的内核镜像内；
    // `frame` 是真实的独占 RAM，仅传给检查模式的接口，不直接访问其内容。探针只借助外层根读取用户映射。
    unsafe {
        first
            .with_active(|| {
                assert!(!is_user_inactive());
                assert_eq!(AddressSpace::new().err(), Some(UnavailableInCurrentMode));
                assert_eq!(
                    second.map_page(SINGLE_SPACE_ADDRESS, Access::Read, 0, &[99]),
                    Err(UnavailableInCurrentMode),
                );
                assert_eq!(
                    second.with_active::<()>(|| panic!("nested user root should not run")),
                    Err(UnavailableInCurrentMode),
                );
                assert_eq!(
                    init_zeroed_frame::<(), UserMappingError>(&mut frame, |_| {
                        panic!("frame initializer should not run")
                    }),
                    Err(UnavailableInCurrentMode),
                );
                assert_eq!(frames.available(), before);
                assert!(!is_user_inactive());
                // 只有外层根映射了该地址，通过用户态读回验证仍在使用外层映射。
                assert_eq!(task.run(), ExitStatus::Exited(73));
            })
            .unwrap();
    }
    assert!(is_user_inactive());
    assert_eq!(frames.available(), before);
}

fn duplicate_when_exhausted(frames: &FrameAllocator, count: usize, read_byte: usize) {
    with_small_pool(frames, count, |pool| {
        // SAFETY: 自检在关中断的 S-mode 启动主核上运行，小池独占借出的真实 RAM 页框。
        let mut space = with_probes(unsafe { AddressSpace::new_in(pool) }.unwrap(), 53);
        assert_eq!(pool.available(), 0);
        let original = space.table.translate(USER_DATA_ADDRESS).unwrap();
        assert_eq!(
            space.map_page(USER_DATA_ADDRESS, Access::Execute, 0, &[99]),
            Err(UserMappingError::AlreadyMapped),
        );
        assert_eq!(space.table.translate(USER_DATA_ADDRESS), Ok(original));
        run_probe(&space, read_byte, USER_DATA_ADDRESS, ExitStatus::Exited(53));
        assert_eq!(pool.available(), 0);
    });
}

/// 验证用户根隔离、同根系统调用往返、访问权限、栈保护和页框回收。
pub fn kernel_test() {
    assert!(is_user_inactive());
    let base = ptr::addr_of!(riscv_user_space_start) as usize;
    let symbol_offset = |label: *const u8| label as usize - base;
    let load = USER_CODE_ADDRESS + symbol_offset(ptr::addr_of!(riscv_user_space_load));
    let read_byte_offset = symbol_offset(ptr::addr_of!(riscv_user_space_read_byte));
    let read_byte = USER_CODE_ADDRESS + read_byte_offset;
    let store = USER_CODE_ADDRESS + symbol_offset(ptr::addr_of!(riscv_user_space_store));
    let code_byte = USER_CODE_ADDRESS + symbol_offset(ptr::addr_of!(riscv_user_space_code_byte));
    let frames = crate::mm::frames();
    let before = frames.available();
    let frames_for_probes = {
        let mut first = new_space(10);
        // 小池复用同一套映射，按已成功建立的地址空间确定容量。
        let frames_for_probes = before - frames.available();
        let mut second = new_space(40);
        first
            .map_page(SINGLE_SPACE_ADDRESS, Access::Read, 0, &[73])
            .unwrap();
        first
            .map_page(EXECUTE_ONLY_ADDRESS, Access::Execute, 0, probe_code())
            .unwrap();
        check_layout(&mut first);
        check_layout(&mut second);
        assert_ne!(first.table.root_address(), second.table.root_address());
        for address in [USER_CODE_ADDRESS, USER_DATA_ADDRESS, STACK_PAGE] {
            assert_ne!(
                first.table.translate(address).unwrap().physical_address,
                second.table.translate(address).unwrap().physical_address,
            );
        }
        for (use_first, exit_code) in [(true, 11), (false, 41), (true, 12), (false, 42)] {
            let space = if use_first { &first } else { &second };
            run_probe(
                space,
                USER_CODE_ADDRESS,
                USER_DATA_ADDRESS,
                ExitStatus::Exited(exit_code),
            );
        }

        run_probe(&first, read_byte, code_byte, ExitStatus::Exited(0x5a));
        run_probe(
            &first,
            read_byte,
            SINGLE_SPACE_ADDRESS,
            ExitStatus::Exited(73),
        );
        check_fault(
            &second,
            read_byte,
            SINGLE_SPACE_ADDRESS,
            ExceptionCause::LoadPageFault,
        );
        run_probe(
            &first,
            EXECUTE_ONLY_ADDRESS + read_byte_offset,
            code_byte,
            ExitStatus::Exited(0x5a),
        );
        check_fault(
            &first,
            read_byte,
            EXECUTE_ONLY_ADDRESS,
            ExceptionCause::LoadPageFault,
        );
        check_fault(
            &first,
            store,
            SINGLE_SPACE_ADDRESS,
            ExceptionCause::StoreAmoPageFault,
        );
        check_fault(
            &first,
            SINGLE_SPACE_ADDRESS,
            SINGLE_SPACE_ADDRESS,
            ExceptionCause::InstructionPageFault,
        );

        for space in [&first, &second] {
            for address in space.table.table_page_addresses() {
                assert_eq!(space.table.translate(address), Err(PagingError::NotMapped));
            }
        }
        let kernel = ptr::addr_of!(skernel) as usize;
        check_fault(&first, load, kernel, ExceptionCause::LoadPageFault);
        check_fault(&first, load, UART_BASE, ExceptionCause::LoadPageFault);
        let root = first.table.root_address();
        check_fault(&first, load, root, ExceptionCause::LoadPageFault);
        check_fault(
            &first,
            store,
            USER_CODE_ADDRESS,
            ExceptionCause::StoreAmoPageFault,
        );
        check_fault(&first, load, STACK_GUARD, ExceptionCause::LoadPageFault);
        check_fault(
            &first,
            USER_DATA_ADDRESS,
            USER_DATA_ADDRESS,
            ExceptionCause::InstructionPageFault,
        );
        check_active_rejections(&first, &mut second, read_byte);
        frames_for_probes
    };
    duplicate_when_exhausted(frames, frames_for_probes, read_byte);
    assert!(is_user_inactive());
    assert_eq!(
        frames.available(),
        before,
        "user address space frames leaked"
    );
    crate::println!("riscv64: user address space kernel tests passed");
}
