//! 通过地址空间的构造和执行接口验证真实用户权限、隔离及页框回收。

use core::{arch::global_asm, ptr, slice};

use super::{Access, AddressSpace, STACK_GUARD, STACK_PAGE, USER_END};
use crate::arch::paging::{sv39_user_root, Error as PagingError, Permissions};
use crate::arch::{
    syscall::ARGUMENT_START,
    trap::{cause::ExceptionCause, cause::TrapCause, TrapContext},
};
use crate::mm::{frame::PAGE_SIZE, UserMappingError};
use crate::task::{ExitStatus, Task};

global_asm!(include_str!("probes.S"));

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

fn new_space(initial: u8) -> AddressSpace {
    let start = ptr::addr_of!(riscv_user_space_start);
    let len = ptr::addr_of!(riscv_user_space_end) as usize - start as usize;
    assert!((1..=PAGE_SIZE).contains(&len));
    // SAFETY: 汇编探针是内核只读区中同一连续片段，首尾均由链接符号标明。
    let code = unsafe { slice::from_raw_parts(start, len) };
    // SAFETY: 自检在 Bare 下的启动主核上执行，启动分配器已初始化并管理真实 RAM。
    let mut space = unsafe { AddressSpace::new(&[]) }.unwrap();
    space
        .map_page(USER_CODE_ADDRESS, Access::ReadExecute, 0, code)
        .unwrap();
    space
        .map_page(USER_DATA_ADDRESS, Access::ReadWrite, 0, &[initial])
        .unwrap();
    space
}

fn check_layout(space: &mut AddressSpace) {
    let text = ptr::addr_of!(skernel) as usize;
    let rodata = ptr::addr_of!(srodata) as usize;
    let data = ptr::addr_of!(sdata) as usize;
    for (address, permissions) in [
        (text, Permissions::READ | Permissions::EXECUTE),
        (rodata, Permissions::READ),
        (data, Permissions::READ | Permissions::WRITE),
        (USER_END, Permissions::READ | Permissions::WRITE),
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
    for address in [
        0,
        USER_CODE_ADDRESS + 1,
        STACK_GUARD,
        STACK_PAGE,
        USER_END,
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
    space: &mut AddressSpace,
    entry: usize,
    probe_arg: usize,
    expected: ExitStatus,
) -> Task {
    let mut context = TrapContext::user(entry, AddressSpace::STACK_TOP);
    context.registers_mut()[ARGUMENT_START] = probe_arg;
    let mut task = Task::new(context);
    let before = crate::mm::frames().available();
    // SAFETY: 测试运行在关中断的启动主核与内核镜像中的启动栈上，Trap 已安装。
    // 任务与 Trap 现场持续可访问；探针通过系统调用或同步故障返回，
    // 任务循环不软件访问表页。
    assert_eq!(unsafe { space.with_active(|| task.run()) }, Ok(expected));
    assert_eq!(sv39_user_root::satp(), 0);
    assert_eq!(crate::mm::frames().available(), before);
    task
}

fn check_fault(space: &mut AddressSpace, entry: usize, addr: usize, cause: ExceptionCause) {
    let task = run_probe(space, entry, addr, ExitStatus::Faulted);
    assert_eq!(task.context().cause(), TrapCause::Exception(cause));
    assert_eq!(task.context().pc(), entry);
    assert_eq!(task.context().bad_addr(), addr);
}

/// 验证两个用户根、同根系统调用往返、监管态权限、栈保护和资源回收。
pub fn kernel_test() {
    assert_eq!(sv39_user_root::satp(), 0);
    let frames = crate::mm::frames();
    let before = frames.available();
    {
        let mut first = new_space(10);
        let mut second = new_space(40);
        first
            .map_page(SINGLE_SPACE_ADDRESS, Access::Read, 0, &[73])
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
            let space = if use_first { &mut first } else { &mut second };
            run_probe(
                space,
                USER_CODE_ADDRESS,
                USER_DATA_ADDRESS,
                ExitStatus::Exited(exit_code),
            );
        }

        let base = ptr::addr_of!(riscv_user_space_start) as usize;
        let load = USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_load) as usize - base);
        let read_byte =
            USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_read_byte) as usize - base);
        let store = USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_store) as usize - base);
        let code_byte =
            USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_code_byte) as usize - base);
        run_probe(&mut first, read_byte, code_byte, ExitStatus::Exited(0x5a));
        run_probe(
            &mut first,
            read_byte,
            SINGLE_SPACE_ADDRESS,
            ExitStatus::Exited(73),
        );
        check_fault(
            &mut second,
            read_byte,
            SINGLE_SPACE_ADDRESS,
            ExceptionCause::LoadPageFault,
        );

        for space in [&first, &second] {
            for address in space.table.table_page_addresses() {
                assert_eq!(space.table.translate(address), Err(PagingError::NotMapped));
            }
        }
        let kernel = ptr::addr_of!(skernel) as usize;
        check_fault(&mut first, load, kernel, ExceptionCause::LoadPageFault);
        check_fault(&mut first, load, USER_END, ExceptionCause::LoadPageFault);
        let root = first.table.root_address();
        check_fault(&mut first, load, root, ExceptionCause::LoadPageFault);
        check_fault(
            &mut first,
            store,
            USER_CODE_ADDRESS,
            ExceptionCause::StoreAmoPageFault,
        );
        check_fault(&mut first, load, STACK_GUARD, ExceptionCause::LoadPageFault);
        check_fault(
            &mut first,
            USER_DATA_ADDRESS,
            USER_DATA_ADDRESS,
            ExceptionCause::InstructionPageFault,
        );
    }
    assert_eq!(
        frames.available(),
        before,
        "user address space frames leaked"
    );
    crate::println!("riscv64: user address space kernel tests passed");
}
