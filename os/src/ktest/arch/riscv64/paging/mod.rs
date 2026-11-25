//! 真实页框上的软件查表、失败回滚，以及临时启用 Sv39 的用户隔离探针。

use core::arch::{asm, global_asm};

use super::csr::satp;
use super::{Error, PageTable, Permissions, Translation};
use crate::arch::riscv64::UART_BASE;
use crate::arch::trap::{cause::ExceptionCause, cause::TrapCause, run_user, TrapContext};
use crate::ktest::support::frames::with_small_pool;
use crate::mm::frame::{FrameAllocator, PAGE_SIZE};
use crate::mm::regions::Region;
use crate::task::{ExitStatus, Task};

global_asm!(include_str!("probes.S"));

extern "C" {
    static riscv_paging_user_start: u8;
    static riscv_paging_user_load: u8;
    static riscv_paging_user_store: u8;
    static riscv_paging_user_end: u8;
}

const USER_CODE: usize = 0x1_0000;
const USER_DATA: usize = 0x2_0000;
const USER_STACK: usize = 0x3_0000;
const UNMAPPED: usize = 0x4_0000;
const NESTED_PROBE: usize = 0x5_0000;

fn new_table(frames: &FrameAllocator) -> PageTable<'_> {
    // SAFETY: 本套件只传入真实启动 RAM，或由存活的真实页框支撑的独占小池；
    // 构建期间使用 Bare 模式，活动作用域另行保留全部表页的监管态同地址映射。
    unsafe { PageTable::new(frames) }.unwrap()
}

fn mapping_and_recycling(frames: &FrameAllocator) {
    let available = frames.available();
    {
        let mut table = new_table(frames);
        assert_eq!(frames.available(), available - 1);
        assert_eq!(table.translate(0), Err(Error::NotMapped));
        // 覆盖同一 L0 表、L0/L1 分界，以及正负规范地址边界。
        let addresses = [
            0,
            PAGE_SIZE,
            1 << 21,
            1 << 30,
            (1 << 38) - PAGE_SIZE,
            !((1 << 38) - 1),
            usize::MAX - PAGE_SIZE + 1,
        ];
        for (index, address) in addresses.into_iter().enumerate() {
            let physical = index * PAGE_SIZE;
            table
                .map(address, physical, Permissions::READ | Permissions::WRITE)
                .unwrap();
            for offset in [0, 1, PAGE_SIZE - 1] {
                assert_eq!(
                    table.translate(address + offset),
                    Ok(Translation {
                        physical_address: physical + offset,
                        permissions: Permissions::READ | Permissions::WRITE,
                    }),
                );
            }
        }
        let before_duplicate = frames.available();
        assert_eq!(
            table.map(0, PAGE_SIZE, Permissions::EXECUTE),
            Err(Error::AlreadyMapped)
        );
        assert_eq!(frames.available(), before_duplicate);
        assert_eq!(table.translate(0).unwrap().physical_address, 0);
        for address in addresses {
            let old = table.translate(address).unwrap();
            assert_eq!(table.unmap(address), Ok(old));
            assert_eq!(table.translate(address), Err(Error::NotMapped));
            assert_eq!(table.unmap(address), Err(Error::NotMapped));
        }
        assert_eq!(
            frames.available(),
            available - 1,
            "empty intermediate tables leaked"
        );
        // 再次映射，验证已回收表页的旧项不会复活；析构还要释放非空页表。
        table.map(0, 0, Permissions::EXECUTE).unwrap();
        assert_eq!(table.translate(PAGE_SIZE), Err(Error::NotMapped));
    }
    assert_eq!(frames.available(), available);
}

fn invalid_inputs(frames: &FrameAllocator) {
    let mut table = new_table(frames);
    let available = frames.available();
    for address in [1 << 38, (1 << 39) - PAGE_SIZE, 1 << 63] {
        assert_eq!(
            table.map(address, 0, Permissions::READ | Permissions::WRITE),
            Err(Error::InvalidVirtualAddress)
        );
        assert_eq!(table.unmap(address), Err(Error::InvalidVirtualAddress));
        assert_eq!(table.translate(address), Err(Error::InvalidVirtualAddress));
    }
    assert_eq!(
        table.map(1, 0, Permissions::READ | Permissions::WRITE),
        Err(Error::UnalignedAddress)
    );
    assert_eq!(
        table.map(0, 1, Permissions::READ | Permissions::WRITE),
        Err(Error::UnalignedAddress)
    );
    assert_eq!(table.unmap(1), Err(Error::UnalignedAddress));
    assert_eq!(
        table.map(0, 1 << 56, Permissions::READ | Permissions::WRITE),
        Err(Error::InvalidPhysicalAddress)
    );
    for permissions in [
        Permissions::WRITE,
        Permissions::USER,
        Permissions::WRITE | Permissions::EXECUTE,
    ] {
        assert_eq!(table.map(0, 0, permissions), Err(Error::InvalidPermissions));
    }
    assert_eq!(frames.available(), available);
    let highest_page = (1 << 56) - PAGE_SIZE;
    table
        .map(0, highest_page, Permissions::EXECUTE | Permissions::USER)
        .unwrap();
    assert_eq!(
        table.translate(PAGE_SIZE - 1).unwrap().physical_address,
        (1 << 56) - 1
    );
}

fn allocation_rollback(frames: &FrameAllocator) {
    with_small_pool(frames, 0, |pool| {
        // SAFETY: 空池不会提供任何地址，也不进行物理内存访问。
        assert!(matches!(
            unsafe { PageTable::new(pool) },
            Err(Error::OutOfMemory)
        ));
    });
    for count in 1..=4 {
        with_small_pool(frames, count, |pool| {
            let mut table = new_table(pool);
            let available = pool.available();
            if count < 3 {
                assert_eq!(
                    table.map(0, 0, Permissions::READ | Permissions::WRITE),
                    Err(Error::OutOfMemory)
                );
                assert_eq!(
                    pool.available(),
                    available,
                    "partially allocated branch leaked"
                );
                assert_eq!(table.translate(0), Err(Error::NotMapped));
            } else {
                table
                    .map(0, 0, Permissions::READ | Permissions::WRITE)
                    .unwrap();
                let after_map = pool.available();
                assert_eq!(
                    table.map(1 << 30, PAGE_SIZE, Permissions::READ | Permissions::WRITE),
                    Err(Error::OutOfMemory)
                );
                assert_eq!(pool.available(), after_map);
                assert_eq!(table.translate(0).unwrap().physical_address, 0);
                assert_eq!(table.translate(1 << 30), Err(Error::NotMapped));
                // 已有两级表时不再申请页框。
                table
                    .map(PAGE_SIZE, PAGE_SIZE, Permissions::READ | Permissions::WRITE)
                    .unwrap();
                table.unmap(0).unwrap();
                assert_eq!(pool.available(), after_map, "shared table freed too early");
                table.unmap(PAGE_SIZE).unwrap();
                assert_eq!(pool.available(), available);
            }
        });
    }
}

fn assert_user_fault(entry: usize, address: usize, cause: ExceptionCause) {
    let mut context = TrapContext::user(entry, USER_STACK + PAGE_SIZE);
    context.registers_mut()[10] = address;
    // SAFETY: 活动页表保护内核和页表存储；固定探针只访问给定用户地址。
    // 故意缺失的映射或权限触发同步用户故障，由已初始化的 trap 返回本栈。
    unsafe { run_user(&mut context) };
    assert_eq!(context.cause(), TrapCause::Exception(cause));
    assert_eq!(context.pc(), entry);
    assert_eq!(context.bad_addr(), address);
}

fn map_kernel(table: &mut PageTable<'_>, kernel: Region) {
    let start = kernel.start & !(PAGE_SIZE - 1);
    let end = (kernel.end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    for address in (start..end).step_by(PAGE_SIZE) {
        // 测试临时保留整个镜像，包括静态堆、启动栈和 trap；不是最终分段权限策略。
        table
            .map(
                address,
                address,
                Permissions::READ | Permissions::WRITE | Permissions::EXECUTE,
            )
            .unwrap();
    }
    table
        .map(UART_BASE, UART_BASE, Permissions::READ | Permissions::WRITE)
        .unwrap();
}

fn map_table_storage(tables: &mut [PageTable<'_>]) {
    // 任一地址空间激活时，所有仍存活的表页都须能按物理地址访问。
    // 建立恒等映射也可能分配表页，因此重复扫描直到一整轮不再新增映射。
    loop {
        let mut changed = false;
        for owner in 0..tables.len() {
            for index in 0..=tables[owner].tables.len() {
                let address = if index == 0 {
                    tables[owner].root.start_address()
                } else {
                    tables[owner].tables[index - 1].start_address()
                };
                for table in &mut *tables {
                    if table.translate(address) == Err(Error::NotMapped) {
                        table
                            .map(address, address, Permissions::READ | Permissions::WRITE)
                            .unwrap();
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn hardware_translation(frames: &FrameAllocator) {
    assert_eq!(satp(), 0, "paging suite must start in bare mode");
    let mut table = new_table(frames);
    let code = frames.alloc().unwrap();
    let data = frames.alloc().unwrap();
    let stack = frames.alloc().unwrap();
    let source = core::ptr::addr_of!(riscv_paging_user_start) as usize;
    let end = core::ptr::addr_of!(riscv_paging_user_end) as usize;
    let load = USER_CODE + core::ptr::addr_of!(riscv_paging_user_load) as usize - source;
    let store = USER_CODE + core::ptr::addr_of!(riscv_paging_user_store) as usize - source;
    assert!(end > source && end - source <= PAGE_SIZE);
    // SAFETY: 三个句柄独占真实 RAM 页，Bare 模式下可直接访问；源是只读探针，
    // 与新分配页不重叠，复制长度已检查。页面在整个活动作用域内保持存活。
    unsafe {
        (code.start_address() as *mut u8).write_bytes(0, PAGE_SIZE);
        core::ptr::copy_nonoverlapping(
            source as *const u8,
            code.start_address() as *mut u8,
            end - source,
        );
        (data.start_address() as *mut u8).write_bytes(0, PAGE_SIZE);
        (stack.start_address() as *mut u8).write_bytes(0, PAGE_SIZE);
        (data.start_address() as *mut usize).write(10);
        // sfence.vma 不同步指令缓存；复制的新代码需单独执行 fence.i。
        asm!("fence.i", options(nostack));
    }

    let kernel = crate::arch::boot::kernel_region().unwrap();
    map_kernel(&mut table, kernel);
    table
        .map(
            USER_CODE,
            code.start_address(),
            Permissions::READ | Permissions::EXECUTE | Permissions::USER,
        )
        .unwrap();
    table
        .map(
            USER_DATA,
            data.start_address(),
            Permissions::READ | Permissions::WRITE | Permissions::USER,
        )
        .unwrap();
    table
        .map(
            USER_STACK,
            stack.start_address(),
            Permissions::READ | Permissions::WRITE | Permissions::USER,
        )
        .unwrap();
    map_table_storage(core::slice::from_mut(&mut table));
    let mut context = TrapContext::user(USER_CODE, USER_STACK + PAGE_SIZE);
    context.registers_mut()[10] = USER_DATA;
    let mut task = Task::new(context);
    // SAFETY: 内核镜像和 UART 保持同地址映射且 U=0；用户仅可访问独占代码、
    // 数据和栈页，所有帧在作用域内存活。测试不修改 satp 或启用中断。
    unsafe {
        table
            .with_active(|| {
                assert_eq!(satp(), (8 << 60) | (table.root.start_address() >> 12));
                // 同时验证活动映射下 UART 和软件查表可用。
                crate::println!("riscv64: Sv39 active");
                assert_eq!(
                    table.translate(USER_DATA).unwrap().physical_address,
                    data.start_address()
                );
                // SAFETY: 用户代码仅修改私有数据和栈，然后通过 exit 返回。
                assert_eq!(task.run(), ExitStatus::Exited(11));
                assert_user_fault(load, UNMAPPED, ExceptionCause::LoadPageFault);
                assert_user_fault(load, kernel.start, ExceptionCause::LoadPageFault);
                assert_user_fault(store, USER_CODE, ExceptionCause::StoreAmoPageFault);
                assert_user_fault(USER_DATA, USER_DATA, ExceptionCause::InstructionPageFault);
            })
            .unwrap();
    }
    assert_eq!(satp(), 0, "previous satp was not restored");
    // SAFETY: 已恢复 Bare 模式，独占页框仍存活；用户执行已经同步结束。
    unsafe {
        assert_eq!((data.start_address() as *const usize).read(), 11);
        assert_eq!(
            ((stack.start_address() + PAGE_SIZE - 8) as *const usize).read(),
            11
        );
    }
    // 解除映射后复用同一虚拟地址，检查下次激活从新的物理页读值。
    table.unmap(USER_DATA).unwrap();
    table
        .map(
            USER_DATA,
            stack.start_address(),
            Permissions::READ | Permissions::WRITE | Permissions::USER,
        )
        .unwrap();
    // SAFETY: 与上文相同的内核及独占用户页映射；探针只读复用后的数据地址。
    unsafe {
        table
            .with_active(|| {
                let mut context = TrapContext::user(load, USER_STACK + PAGE_SIZE);
                context.registers_mut()[10] = USER_DATA;
                run_user(&mut context);
                assert_eq!(
                    context.cause(),
                    TrapCause::Exception(ExceptionCause::UserEnvironmentCall)
                );
                assert_eq!(context.registers()[10], 0, "remapped user data");
            })
            .unwrap();
    }
    assert_eq!(satp(), 0);
    // 先销毁所有映射，再允许用户页面的 Frame 句柄归还物理页。
    drop(table);
}

fn nested_activation(frames: &FrameAllocator) {
    let bare = satp();
    assert_eq!(bare, 0, "nested paging test must start in bare mode");
    let data = [frames.alloc().unwrap(), frames.alloc().unwrap()];
    let values = [0xa5, 0x5a];
    let mut tables = [new_table(frames), new_table(frames)];
    let kernel = crate::arch::boot::kernel_region().unwrap();
    for ((table, frame), value) in tables.iter_mut().zip(&data).zip(values) {
        let address = frame.start_address();
        // SAFETY: Bare 模式下可直接写入这个仍由 frame 独占且按页对齐的真实 RAM 页。
        unsafe { (address as *mut usize).write(value) };
        map_kernel(table, kernel);
        table.map(NESTED_PROBE, address, Permissions::READ).unwrap();
    }
    map_table_storage(&mut tables);
    let [outer, inner] = &tables;

    // SAFETY: 目标表对内核镜像、启动栈和 UART 保持同地址映射；启动主核在 S-mode 关中断执行。
    let rejected = unsafe {
        outer.with_active_expecting::<()>(bare, || panic!("mismatched satp must not run"))
    };
    assert_eq!(rejected, Err(Error::ActivationFailed));
    assert_eq!(
        satp(),
        bare,
        "satp was not restored after readback mismatch"
    );

    // SAFETY: 自检在主核 S-mode 关中断执行。两张表对内核镜像、UART 和双方的
    // 表页保持相同映射，且在外层退出前一直存活。探针地址在两张表中均对齐、
    // 只读，映射的独占页已初始化并保持存活；探针通过汇编访存，不形成 Rust 引用。
    unsafe {
        let read_probe = || {
            let value: usize;
            asm!(
                "ld {value}, 0({address})",
                value = out(reg) value,
                address = in(reg) NESTED_PROBE,
                options(readonly, nostack),
            );
            value
        };
        outer
            .with_active(|| {
                let previous = satp();
                assert_eq!(previous >> 60, 8);
                assert_eq!(read_probe(), values[0], "outer probe before nesting");
                inner
                    .with_active(|| {
                        assert_ne!(satp(), previous, "inner page table was not activated");
                        assert_eq!(read_probe(), values[1], "inner probe");
                    })
                    .unwrap();
                assert_eq!(satp(), previous, "outer satp was not restored");
                assert_eq!(read_probe(), values[0], "outer probe after nesting");
            })
            .unwrap();
    }
    assert_eq!(satp(), bare, "bare satp was not restored");
}

/// 验证 Sv39 软件映射及真实地址翻译，每轮返回前恢复 Bare 模式并释放全部页框。
pub fn kernel_test() {
    let frames = crate::mm::frames();
    let available = frames.available();
    mapping_and_recycling(frames);
    invalid_inputs(frames);
    allocation_rollback(frames);
    hardware_translation(frames);
    nested_activation(frames);
    assert_eq!(frames.available(), available, "paging frames leaked");
    crate::println!("riscv64: Sv39 paging kernel tests passed");
}
