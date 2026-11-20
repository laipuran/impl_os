//! 通过地址空间的构造和执行接口验证真实用户权限、隔离及页框回收。

use core::{arch::global_asm, ptr, slice};

use super::{
    csr, Error, PageKind, PagingError, Permissions, UserAddressSpace, RW, RX, STACK_GUARD,
    STACK_PAGE, UART_BASE,
};
use crate::arch::trap::{cause::ExceptionCause, cause::TrapCause, TrapContext};
use crate::mm::frame::PAGE_SIZE;
use crate::task::{ExitStatus, Task};

global_asm!(include_str!("probes.S"));

extern "C" {
    static riscv_user_space_start: u8;
    static riscv_user_space_load: u8;
    static riscv_user_space_store: u8;
    static riscv_user_space_end: u8;
    static skernel: u8;
    static srodata: u8;
    static sdata: u8;
}

const USER_CODE_ADDRESS: usize = 0x1_0000;
const USER_DATA_ADDRESS: usize = 0x2_0000;

fn new_space(initial: u8) -> UserAddressSpace {
    let start = ptr::addr_of!(riscv_user_space_start);
    let len = ptr::addr_of!(riscv_user_space_end) as usize - start as usize;
    assert!((1..=PAGE_SIZE).contains(&len));
    // SAFETY: 汇编探针是内核只读区中同一连续片段，首尾均由链接符号标明。
    let code = unsafe { slice::from_raw_parts(start, len) };
    // SAFETY: 自检在 Bare 下的启动主核上执行，启动分配器已初始化并管理真实 RAM。
    let mut space = unsafe { UserAddressSpace::new() }.unwrap();
    space
        .map_page(USER_CODE_ADDRESS, PageKind::Code, code)
        .unwrap();
    space
        .map_page(USER_DATA_ADDRESS, PageKind::Data, &[initial])
        .unwrap();
    space
}

fn check_layout(space: &mut UserAddressSpace) {
    let text = ptr::addr_of!(skernel) as usize;
    let rodata = ptr::addr_of!(srodata) as usize;
    let data = ptr::addr_of!(sdata) as usize;
    for (address, permissions) in [
        (text, RX),
        (rodata, Permissions::READ),
        (data, RW),
        (UART_BASE, RW),
    ] {
        let entry = space.table.translate(address).unwrap();
        assert_eq!(entry.physical_address, address);
        assert_eq!(entry.permissions, permissions);
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
        UART_BASE,
        text,
    ] {
        assert_eq!(
            space.map_page(address, PageKind::Data, &[]),
            Err(Error::InvalidUserAddress),
        );
    }
    assert_eq!(
        space.map_page(USER_DATA_ADDRESS, PageKind::Data, &[99]),
        Err(Error::Paging(PagingError::AlreadyMapped)),
    );
    assert_eq!(
        space.map_page(
            USER_DATA_ADDRESS + PAGE_SIZE,
            PageKind::Data,
            &[0; PAGE_SIZE + 1],
        ),
        Err(Error::InitialDataTooLarge),
    );
    assert_eq!(crate::mm::frames().available(), before);
}

fn run_probe(
    space: &mut UserAddressSpace,
    entry: usize,
    address: usize,
    expected: ExitStatus,
) -> Task {
    let mut context = TrapContext::user(entry, space.stack_top());
    context.registers_mut()[10] = address;
    let mut task = Task::new(context);
    // SAFETY: 启动主核保持关中断，Trap 已安装，任务驻留在启动栈。
    // 入口和栈均对齐；探针会通过系统调用或同步故障返回。
    assert_eq!(unsafe { space.run(&mut task) }, Ok(expected));
    assert_eq!(csr::satp(), 0);
    task
}

fn check_fault(space: &mut UserAddressSpace, entry: usize, addr: usize, cause: ExceptionCause) {
    let task = run_probe(space, entry, addr, ExitStatus::Faulted);
    assert_eq!(task.context().cause(), TrapCause::Exception(cause));
    assert_eq!(task.context().pc(), entry);
    assert_eq!(task.context().bad_addr(), addr);
}

/// 验证两个用户根、同根系统调用往返、监管态权限、栈保护和资源回收。
pub fn kernel_test() {
    assert_eq!(csr::satp(), 0);
    let frames = crate::mm::frames();
    let before = frames.available();
    {
        let mut first = new_space(10);
        let mut second = new_space(40);
        check_layout(&mut first);
        assert_ne!(
            first.table.root.start_address(),
            second.table.root.start_address()
        );
        assert_ne!(
            first
                .table
                .translate(USER_DATA_ADDRESS)
                .unwrap()
                .physical_address,
            second
                .table
                .translate(USER_DATA_ADDRESS)
                .unwrap()
                .physical_address,
        );
        run_probe(
            &mut first,
            USER_CODE_ADDRESS,
            USER_DATA_ADDRESS,
            ExitStatus::Exited(11),
        );
        run_probe(
            &mut second,
            USER_CODE_ADDRESS,
            USER_DATA_ADDRESS,
            ExitStatus::Exited(41),
        );
        run_probe(
            &mut first,
            USER_CODE_ADDRESS,
            USER_DATA_ADDRESS,
            ExitStatus::Exited(12),
        );
        run_probe(
            &mut second,
            USER_CODE_ADDRESS,
            USER_DATA_ADDRESS,
            ExitStatus::Exited(42),
        );

        let base = ptr::addr_of!(riscv_user_space_start) as usize;
        let load = USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_load) as usize - base);
        let store = USER_CODE_ADDRESS + (ptr::addr_of!(riscv_user_space_store) as usize - base);
        let kernel = ptr::addr_of!(skernel) as usize;
        check_fault(&mut first, load, kernel, ExceptionCause::LoadPageFault);
        check_fault(&mut first, load, UART_BASE, ExceptionCause::LoadPageFault);
        let root = first.table.root.start_address();
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
