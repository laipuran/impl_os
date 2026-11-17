//! LoongArch 64 单核关中断环境下的 PLV0 / PLV3 整数上下文切换。
//!
//! 寄存器与暂存 CSR 约定：
//! - 占用 `SAVE0..SAVE3`（CSR `0x30..0x33`）：`SAVE0`/`SAVE1` 供异常入口暂存 `$t0`/`$sp`，
//!   `SAVE2` 存放用户态切换栈帧指针，`SAVE3` 保存内核态的 `$tp`（`$r2`）；
//! - 全局中断与浮点/向量扩展（`EUEN` 中的 FPE/SXE/ASXE）始终保持关闭；
//! - 启动阶段建立的 `DMW0` 窗口仅允许 PLV0 访问，运行用户态代码需由调用方提供 PLV3 映射。

use core::arch::{asm, global_asm};

pub mod cause;
use cause::TrapCause;

/// 捕获 32 个通用寄存器（`$r0..$r31`）与异常控制状态的上下文结构。
///
/// - `gpr` 按架构寄存器编号 `$r0..$r31` 索引，其中 `$r3` 为栈指针 `$sp`，`$r2` 为线程指针 `$tp`。
/// - 普通异常保存 `ERA`、`PRMD`、`ESTAT` 和 `BADV`。
/// - TLB 重填（refill）异常则从 `TLBRERA`、`TLBRBADV` 读取 PC 与故障地址，将 `TLBRPRMD.PWE`
///   归一化到 `prmd` 的第 3 位，并在 `estat` 的第 63 位打上软件重填标记（因为硬件触发 refill 时不会更新 `ESTAT.Ecode`）。
#[repr(C)]
pub struct TrapContext {
    gpr: [usize; 32],
    era: usize,
    prmd: usize,
    estat: usize,
    badv: usize,
}

impl TrapContext {
    /// 构造初始 PLV3 用户态上下文（关闭中断与硬件监视点）。
    ///
    /// `entry` 为 4 字节对齐的用户程序入口地址，`stack` 为 16 字节对齐的用户栈顶地址。
    /// 本构造函数仅初始化寄存器数值，不检查地址有效性，也不建立页表映射。
    #[allow(dead_code)]
    pub fn user(entry: usize, stack: usize) -> Self {
        let mut gpr = [0; 32];
        gpr[3] = stack;
        Self {
            gpr,
            era: entry,
            prmd: 3,
            estat: 0,
            badv: 0,
        }
    }

    /// 返回按 `$r0..$r31` 排列的通用寄存器只读切片。
    #[allow(dead_code)]
    pub fn registers(&self) -> &[usize; 32] {
        &self.gpr
    }

    /// 返回按 `$r0..$r31` 排列的通用寄存器可变切片（修改 `gpr[0]` 在硬件恢复时仍恒为 0）。
    #[allow(dead_code)]
    pub fn registers_mut(&mut self) -> &mut [usize; 32] {
        &mut self.gpr
    }

    /// 返回捕获的异常返回地址（普通异常为 `ERA`，TLB 重填异常为清零低位后的 `TLBRERA`）。
    #[allow(dead_code)]
    pub fn pc(&self) -> usize {
        self.era
    }

    /// 设置下次返回时的异常程序计数器（`ERA` / `TLBRERA`）。
    ///
    /// 本方法仅更新上下文中保存的数值，不校验地址对齐、映射或执行权限；
    /// 若期望恢复后正常取指执行，调用方须在恢复前保证目标地址满足 4 字节指令对齐及对应特权级的映射与内存安全条件（故意传入未映射地址以测试异常/重填捕获的场景除外，详见 [`run_user`]）。
    #[allow(dead_code)]
    pub fn set_pc(&mut self, pc: usize) {
        self.era = pc;
    }

    /// 返回被打断时的栈指针（`$r3` / `$sp`）。
    #[allow(dead_code)]
    pub fn sp(&self) -> usize {
        self.gpr[3]
    }

    /// 将保存的 `estat` 解码为类型化的 [`TrapCause`]（第 63 位的软件重填标记优先于 `Ecode`）。
    #[allow(dead_code)]
    pub fn cause(&self) -> TrapCause {
        TrapCause::from_estat(self.estat)
    }

    /// 返回保存的 `estat` 值（若来自 TLB 重填入口，第 63 位会被置为 1）。
    #[allow(dead_code)]
    pub fn raw_cause(&self) -> usize {
        self.estat
    }

    /// 返回保存的 `prmd` 值（若来自 TLB 重填入口，则是将 `PWE` 移至第 3 位后的归一化 `TLBRPRMD`）。
    #[allow(dead_code)]
    pub fn raw_status(&self) -> usize {
        self.prmd
    }

    /// 返回出错虚拟地址（普通异常为 `BADV`，TLB 重填异常为 `TLBRBADV`）。
    #[allow(dead_code)]
    pub fn bad_addr(&self) -> usize {
        self.badv
    }
}

// 编译期静态断言：确保 `TrapContext` 大小与字段偏移满足汇编 16 字节栈对齐及 12 位有符号立即数寻址要求。
const _: () = {
    assert!(core::mem::size_of::<usize>() == 8);
    assert!(core::mem::offset_of!(TrapContext, gpr) % 8 == 0);
    assert!(core::mem::size_of::<TrapContext>() % 16 == 0);
    assert!(core::mem::size_of::<TrapContext>() <= 2047);
    assert!(core::mem::offset_of!(TrapContext, badv) <= 2047);
};

global_asm!(
    include_str!("entry.S"),
    include_str!("return.S"),
    frame_size = const core::mem::size_of::<TrapContext>(),
    gpr = const core::mem::offset_of!(TrapContext, gpr),
    word = const core::mem::size_of::<usize>(),
    era = const core::mem::offset_of!(TrapContext, era),
    prmd = const core::mem::offset_of!(TrapContext, prmd),
    estat = const core::mem::offset_of!(TrapContext, estat),
    badv = const core::mem::offset_of!(TrapContext, badv),
);

extern "C" {
    fn __loongarch_trap_entry();
    fn __loongarch_refill_entry();
    fn __loongarch_run_user(context: *mut TrapContext);
}

/// 安装 4 KiB 对齐的普通异常入口（`EENTRY`）与 TLB 重填物理入口（`TLBRENTRY`），并初始化 `SAVE0..SAVE3`。
///
/// 关闭全局中断（`CRMD.IE`）、局部中断使能（`ECFG`）以及浮点/向量扩展（`EUEN`），并将当前内核 `$tp` 备份至 `SAVE3`。
/// 必须在启动阶段由主核在 PLV0、内核栈有效且无任何活跃用户态切换时调用一次（早于任何 [`run_user`] 调用），
/// 不可在存在活跃用户态切换期间重复调用（会覆盖 `SAVE2` 切换栈帧指针与 `SAVE3` 内核线程指针基准）。
pub fn init() {
    // SAFETY: 启动阶段仅主核在 PLV0 下执行；两个汇编异常入口均已按 4096 字节对齐且位于常驻内核代码段。
    unsafe {
        asm!(
            "li.w {tmp}, 4",
            "csrxchg $zero, {tmp}, 0x0",
            "csrwr $zero, 0x4",
            "csrwr $zero, 0x2",
            "csrwr $zero, 0x30",
            "csrwr $zero, 0x31",
            "csrwr $zero, 0x32",
            "move {tmp}, $tp",
            "csrwr {tmp}, 0x33",
            "csrwr {vector}, 0xc",
            "csrwr {refill}, 0x88",
            "csrwr $zero, 0x8a",
            "ibar 0",
            tmp = out(reg) _,
            vector = inout(reg) __loongarch_trap_entry as *const () as usize => _,
            refill = inout(reg) (
                __loongarch_refill_entry as *const () as usize - 0x9000_0000_0000_0000
            ) => _,
            options(nostack),
        );
    }
}

/// 降权进入 PLV3 执行 `context`，当用户态触发任何异常或 TLB 重填时保存完整整数现场并返回。
///
/// 进入用户态前会将 `context.prmd` 强制设为 `3`（`PPLV = 3, PIE = 0, PWE = 0`），防止外部传入的特权位泄漏。
/// 往返过程中内核的被调用者保存寄存器（`$fp`、`$s0..$s8`）、`$ra`、`$tp`、`$r21` 与 `$sp` 均保持不变。
///
/// # Safety
///
/// - 调用前当前核必须已在 PLV0 执行 [`init`]，且保持单核、关中断、无嵌套的用户态切换。
/// - `context` 以及内核代码、当前内核栈和所有内核态数据在整个切换与异常返回期间必须持续有效、可通过 `DMW0` 在 PLV0 下安全访问，且 `context` 由当前调用独占。
/// - 本函数仅切换特权级与寄存器，不建立页表映射，也不提供用户态与内核态的内存隔离（`DMW0` 虽限制为 PLV0，但无法阻止 PLV3 通过 `DMW1` 等窗口别名访问同一物理 RAM）。
/// - 若需用户程序正常取指与访存，调用方必须提供 4 字节对齐的用户入口 PC、16 字节对齐的用户栈及对应的 PLV3 映射；若调用方故意传入未映射的用户入口或访存地址以测试异常捕获，硬件触发的异常或 TLB 重填会保存现场并安全返回给调用方（不会自动填表或重试）。无论何种情况，所执行的用户代码都必须受控，严禁篡改任何内核代码、内核栈、静态数据、分配器状态或 `context` 自身。
#[allow(dead_code)]
pub unsafe fn run_user(context: &mut TrapContext) {
    context.prmd = 3;
    // SAFETY: 调用者保证单核关中断环境、内核与 `context` 映射有效且用户代码受控；汇编先在内核栈保存被调用者寄存器并写入 `SAVE2`，再执行 `ertn`。
    unsafe { __loongarch_run_user(context) };
}

#[no_mangle]
extern "C" fn __loongarch_handle_kernel(context: &mut TrapContext) {
    crate::trap::handle_kernel(context);
}

#[cfg(feature = "ktest")]
#[path = "../../../ktest/arch/loongarch64/mod.rs"]
pub(crate) mod tests;
