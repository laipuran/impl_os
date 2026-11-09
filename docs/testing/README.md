# 开发检查

## 命令覆盖范围

| 命令 | 检查范围 | 不覆盖 |
| --- | --- | --- |
| 容器内 `just check` | Rust 格式、双架构 Clippy | 链接产物、启动行为、内核功能 |
| 容器内 `just build` / `make all` | 两架构 ELF 构建与导出 | 启动及运行时语义 |
| 宿主 `just judge-build` | 原始比赛镜像断网，过滤隐藏文件和目录，以空 Cargo 缓存及独立构建目录构建双架构 ELF | 启动、评分 |
| 容器内 `just run rv` / `just run la` | 手动观察启动日志和进程退出 | 官方用户程序的正确性 |
| 容器内 `just test [rv\|la] [release\|debug]` | 构建启用 `ktest` 的 ELF 并执行全部已接入内核自检；覆盖 trap、heap、syscall ABI、任务执行循环、物理区间、页框分配、FDT 内存解析及 LA 启动表 | 未实现的测试功能、官方用户程序 |

当前有统一的启动期内核自检入口，通过静态具名测试组列表组织各模块测试，没有自动注册、名称筛选或官方测试集运行入口。格式和构建检查不能替代运行时功能测试。

`judge-build` 在容器临时目录中复制源码，排除隐藏文件和目录、本地 `target/`、`artifacts/` 及根目录已有 ELF；只将成功构建的两个 ELF 复制回仓库。原始镜像必须预先存在，并提供固定工具链及两个目标的标准库；命令不会拉取镜像或安装工具链。

## 当前运行行为

正常启动时，共享入口清零 BSS，初始化日志、trap 和内核堆，再建立并输出物理内存地图，将可用区间交给页框分配器，输出总页数和空闲页数，不执行试分配或回收验证。内核随后输出架构名称；启用自检时先执行测试，最后调用 `arch::shutdown()`。

RISC-V 使用 SBI System Reset 关机，固件支持该扩展时会退出 QEMU；LoongArch 尚未实现关机，输出提示后停放 CPU，需要手动退出 QEMU。panic 也会输出错误并停放 CPU，因此不能仅凭进程仍在运行就判断启动成功，必须检查串口输出。

## 统一内核自检入口

命令格式为 `just test [架构] [构建模式]`，默认架构 `rv`、模式 `release`。该命令统一启用 Cargo feature `ktest`，经现有 Make 构建入口导出 ELF，再调用 QEMU；`MEM`、`SMP` 环境变量与 `just run` 一致。一次运行执行所选架构的全部已接入测试；验证双架构时分别运行两个 QEMU 实例。

`ktest` 只控制测试代码，不控制 trap、syscall ABI 或内存管理等正式能力。共享入口先正常初始化内核，再调用 `os/src/ktest/mod.rs` 中的 `run`，借用 `mm::init(boot_arg)` 返回的页框分配器；默认构建不编译测试入口、模块自检或汇编探针。这里使用 `#[cfg(feature = "ktest")]`，不是 Rust 的 `#[cfg(test)]` 或自定义 `#[cfg(ktest)]`，也不依赖 Cargo 测试 harness。

自检代码放在所属模块的 `tests/` 子目录，由父模块用同一个 `ktest` feature 控制；仅供自检使用的汇编也放在该子目录。新增独立测试组时，在 `ktest/mod.rs` 的 `SUITES` 列表中增加一个 `Suite { name, run }`，其中 `run` 接收页框分配器的共享引用且无返回值；无需使用分配器的现有测试可通过 `|_| kernel_test()` 适配。依赖已有测试组受控执行环境的辅助测试由该组调用，不单独注册；syscall ABI 与任务测试的接入见下文。列表顺序就是执行顺序；通用执行循环在调用前输出测试组名称，只有所有测试组返回后才输出总成功标记。无需修改 `kernel_main` 或执行循环，不新增测试专用 feature 或 Just recipe。测试依赖的基础能力应由正常初始化提供，不能把另一组自检当作初始化入口。

测试在启动 CPU 上关闭中断、依次执行，每组测试返回前必须恢复临时映射、机器状态并释放测试资源。失败直接走现有 panic 停机，不捕获后继续执行；故意触发致命异常或无法恢复状态的测试应在独立 QEMU 实例中验证，不加入这条顺序执行路径。

## Trap 内核测试

在开发容器内运行：

```sh
just test
just test la
```

两者分别构建启用 `ktest` 的 RISC-V / LoongArch release ELF，再启动 QEMU。trap 内核测试完成后输出 `<架构>: trap kernel tests passed`；只有全部测试完成，统一入口才输出 `<架构>: all kernel tests passed`。断言失败走 panic，不输出总成功标记。LoongArch 成功后仍停放，需 `Ctrl-a`、`x` 退出；自动运行时的超时本身既不是成功也不是失败，必须检查串口的总成功标记。

验证内容：

- 内核 breakpoint 的整数上下文保存/恢复，以及处理期间的同步嵌套异常。LoongArch handler 修改全部非零、非 SP GPR 后，汇编逐个检查恢复值和原始 SP，再恢复探针调用者，避免探针自身恢复掩盖 trap 恢复错误。
- 用户态 `ecall` / `syscall` 返回内核，验证所有整数寄存器、异常 PC、原因和先前特权级。
- 修改用户 PC、整数寄存器和栈指针后重新进入，确认恢复的是修改后的上下文。
- 独立汇编包装经 C 调用边界执行 Rust `run_user`，在首次进入和修改现场后重入时检查内核调用 ABI。RISC-V 检查 s0–s11、有效的原始 gp/tp 和 SP；LoongArch 检查 fp/s0–s8、原始 tp/r21 和 SP。检查先于包装器自身恢复，返回控制流证明能够回到调用者，不要求 caller-saved 寄存器保持原值。
- 输入恶意特权状态位仍只能进入 U/PLV3；实际执行特权指令必须异常。
- 浮点关闭时执行浮点指令必须异常，而不是静默破坏未保存状态。
- LoongArch 内核嵌套 refill、用户未映射取指及其后的正常 syscall；临时 DMW 修改同步失效缓存翻译。
- 原因解码覆盖中断/异常编号空间分离、未知编号保留；LoongArch 额外覆盖多位待处理中断、异常子码及 refill 标记优先于旧 ESTAT。解码断言直接接入 `ktest`，不依赖禁用的 Cargo 测试 harness。

debug 模式直接使用 `just test rv debug` 或 `just test la debug`。自检映射只供固定可信探针使用，不提供进程隔离。恢复正常产物使用 `just build`；默认构建不启用自检。

致命诊断应在独立 QEMU 实例中验证，不能加入顺序自检。可在 debug 内核安装 trap 后用 GDB 将 PC 改为零，分别观察 RISC-V instruction access fault 和 LoongArch TLB refill 的 panic 输出及停机路径；默认日志级别下也必须能看到原因、PC、保存状态、原始原因、故障地址和原始 SP。

LoongArch 的 Rust 可调用汇编函数提供 ELF `FUNC` 类型和函数大小。构建 trap 内核测试后，可用 `readelf -Ws kernel-la` 检查 `__loongarch_run_user`、`__loongarch_kernel_test_kernel`、`__loongarch_kernel_test_nested`、`__loongarch_kernel_test_refill` 的类型及范围；GDB 中 `disassemble __loongarch_run_user` 应止于恢复路径的 `ertn`，不包含后续异常入口前的对齐填充。

## Heap 内核测试

在容器内执行 `just test rv` 或 `just test la`，统一入口会在 trap 测试后执行 heap 测试。该测试验证 `Box` 的页对齐、`Vec` 扩容与数据保持、超过堆总容量的 `try_reserve_exact` 返回错误，以及对象销毁和反复申请后全局堆实际占用恢复。

布局和耗尽检查使用单独的、按 8 KiB 对齐的 8 KiB 静态内存池，不耗尽全局堆。测试检查不同大小与对齐的分配互不覆盖、写入数据保持、满堆返回失败、释放后的块复用，以及两个 4 KiB 伙伴块释放后能够满足 8 KiB 申请。全部通过后输出 `<架构>: heap kernel tests passed`，最后仍以统一入口的总成功标记为准。

独立堆耗尽检查不触发全局分配失败处理；实际不可恢复的分配失败会 panic 停机，必须在独立 QEMU 实例中验证，不能加入顺序自检。

## 物理内存地图检查

内存地图检查分为模块自检和真实启动检查。前者验证解析及区间算法，后者验证平台参数传递、地址转换和实际内存占用。两者均通过后，才能确认启动信息已正确接入；这些检查不覆盖用户隔离或 LoongArch 通用 UEFI 启动。页框分配的检查见下节。

两架构的 `ktest` 均执行 `physical regions` 和 `FDT memory` 测试组。区间测试覆盖合并、扣除、页对齐、溢出和无可用页，并穷举八个页面的保留组合。FDT 测试覆盖多 RAM 区间、一或两个 cell、禁用节点、静态保留区、initrd，以及截断、重叠和不支持的描述等错误输入。LoongArch 额外执行 `boot tables` 测试，检查 GUID 查找、initrd 描述、缺失或重复表项、截断、溢出和 DMW 地址转换。

在开发容器内运行以下命令，检查实际 RAM 区间是否随配置变化：

| 命令 | 预期 `RAM` 区间 |
| --- | --- |
| `MEM=128M just run rv` | `[0x80000000, 0x88000000)` |
| `MEM=1G just run rv` | `[0x80000000, 0xc0000000)` |
| `MEM=768M just run la` | `[0, 0x10000000)`、`[0x80000000, 0xa0000000)` |
| `MEM=1G just run la` | `[0, 0x10000000)`、`[0x80000000, 0xb0000000)` |

`usable` 必须位于 RAM 内、按页对齐，且不与 `reserved` 重叠。保留区必须覆盖整个内核和 FDT；LoongArch 还必须保留前 1 MiB 启动块。FDT 和内核结束地址随启动环境及构建产物变化，应检查覆盖关系，而非固定这些边界。LoongArch 的内核加载地址为 `0x90000000`，128 MiB 等小内存配置无法容纳内核，因此回归使用 768 MiB 及以上配置。

带 initrd 的检查使用文档文件作为字节载荷，只验证其整个物理区间被保留，不解析文件内容：

```sh
MEM=128M just run rv -initrd docs/architecture/boot.md
MEM=3G just run la -initrd docs/architecture/boot.md
```

LoongArch 使用 3 GiB 是为了满足 QEMU 10.0.2 的加载器检查：该加载器将 initrd 的物理末端与 `-m` 字节数比较，而不是与高端 RAM 的末端比较。这个配置要求来自加载器，不是内核堆或 initrd 本身需要 3 GiB。

启动表或设备树解析失败时，内核必须 panic 停机，不得输出正常启动或自检总成功标记。

### 注入不支持的启动内存描述

故障注入必须在独立 QEMU 实例中执行，不加入顺序自检。下面在 Linux 宿主机的仓库根目录运行，要求已有开发镜像和 `device-tree-compiler` 提供的 `fdtput`。先导出一份 128 MiB 配置的设备树副本，再加入没有静态地址的动态保留区；原始设备树和内核源码均不修改。

```sh
mkdir -p artifacts/memory-negative
just container qemu-system-riscv64 -machine virt,dumpdtb=artifacts/memory-negative/base.dtb -m 128M -smp 1 -nographic
cp artifacts/memory-negative/base.dtb artifacts/memory-negative/dynamic.dtb
fdtput -p -t x artifacts/memory-negative/dynamic.dtb /reserved-memory '#address-cells' 2
fdtput -t x artifacts/memory-negative/dynamic.dtb /reserved-memory '#size-cells' 2
fdtput artifacts/memory-negative/dynamic.dtb /reserved-memory ranges
fdtput -p -t x artifacts/memory-negative/dynamic.dtb /reserved-memory/dynamic size 0 1000
just container just build rv release ktest
just container timeout 5s qemu-system-riscv64 -machine virt -bios default -kernel kernel-rv -m 128M -smp 1 -nographic -no-reboot -dtb artifacts/memory-negative/dynamic.dtb
```

最后一个命令应输出包含 `memory initialization failed` 和 `dynamic reserved-memory allocation is unsupported` 的 panic，随后停放 CPU。日志不得出现 `booted on riscv64` 或 `riscv64: all kernel tests passed`。`timeout` 返回 124 是停放后的预期结果，但必须同时核对上述输出，不能单凭超时判定通过。测试副本保留在被忽略的 `artifacts/memory-negative/`，后续普通启动不传入 `-dtb` 即不受注入影响。

## 页框分配检查

页框分配器以位图管理 `MemoryMap::usable()` 返回的区间，每页固定 4 KiB；元数据来自现有静态堆，每 GiB 可用 RAM 约需 32 KiB 位图，另有区间元数据。堆不能容纳位图时初始化返回错误，启动流程 panic 停机。分配和回收本身不申请堆内存。

分配器由启动流程持有，不是全局分配器；使用者通过借用分配器取得独占句柄，句柄销毁时自动回收页框。只支持关闭中断的启动 CPU，不支持连续多页分配，也不建立映射或清零页面。使用物理地址建立映射或裸指针访问时，必须在句柄销毁前结束访问并移除映射。

两架构的 `just test` 均执行 `physical frames` 测试组，使用不解引用的合成物理地址验证地址零、不连续 RAM 和保留区跳过、分配唯一性、耗尽返回 `None`、释放复用、作用域回收、位图字边界及尾部无效位、多轮耗尽和回收，以及八页的全部保留组合（含没有可用页的情况）。全部 RAM 被保留或不足一页时，分配器应成功初始化，总页数和空闲页数均为零，多次分配均返回 `None`。额外验证可用区间未按 4 KiB 对齐时分配器拒绝初始化，以及超大位图无法从堆分配时返回错误。测试不耗尽真实页框池。

地图输入校验由 `physical regions` 测试组覆盖：空或逆序的单个区间、缺失 RAM 描述被拒绝，重叠或相邻区间会排序并合并；扣除保留区并对齐后没有完整可用页时，返回有效地图且 `usable` 为空，包括最高地址附近对齐溢出的情况。`MemoryMap` 隐藏内部区间，只提供只读访问和可用区间的所有权移交；页框分配器依赖其已建立的单个区间非空、排序及不重叠不变量，不要求可用区间列表非空。

`physical frames` 测试组还借用正常启动创建的分配器，尝试分配一个真实页框。有空闲页时检查页数递减、物理地址页对齐且位于可用区间，并在句柄离开作用域后检查空闲页数恢复；没有空闲页时检查分配返回 `None`。这项检查仅在启用 `ktest` 时执行，不会读写页面内容，不能证明物理页映射或清零行为。全部自检通过后仍以统一入口的总成功标记为准。

## Syscall ABI 内核测试

同一组 `just test` 命令也验证 Linux syscall ABI。测试位于 `os/src/syscall/tests/`，由 trap 自检在其受控用户执行环境中调用：RISC-V 使用 bare 模式，依赖固件 PMP 允许访问探针与栈；LoongArch 使用临时 PLV3 低地址 DMW1 别名，并在返回统一测试入口前由 trap 自检恢复。两者均不提供用户隔离，也不注册依赖另一套件遗留状态的独立套件。

- 固定用户探针连续执行三次 syscall，检查调用号和全部六个参数，包含高 32 位非零、最高位为一及负数的原始机器字。
- 正常返回仅修改 a0 和 PC，保留其余整数寄存器及异常元数据；用户将超过 32 位的成功值和负 errno 复制到下一次调用参数，验证实际可见结果和四字节 PC 推进，最后到达指定 breakpoint。
- 非 syscall 的 breakpoint、内核 syscall，以及 LoongArch 带残留 syscall Ecode 的 refill 均被拒绝，且现场不变。内核 syscall 和残留 Ecode 场景使用构造的现场检查，不实际触发致命内核异常。
- 在已捕获的用户 syscall 现场上构造 PC 边界：可表示的四字节推进成功，溢出时拒绝完成并保持 PC、整数寄存器及异常信息不变；不实际执行边界地址。

成功时额外输出 `<架构>: syscall ABI tests passed`，全部套件完成仍以总成功标记为准。

## 任务执行内核测试

同一组 `just test` 命令也通过 `os/src/task/tests/` 中的真实用户探针验证 `Task::run`。测试由 trap 自检在 syscall ABI 测试后调用，复用其可信映射和独占用户栈；LoongArch 在恢复 DMW1 前完成全部任务执行，不增加独立测试组或默认启动的用户映射。

- 连续未知 syscall 在用户态观察到 `-ENOSYS` 并继续执行；包含高 32 位非零、低 32 位为 `exit` 的调用号，防止错误截断调用号。
- `exit` 与 `exit_group` 只保存参数低八位，保留退出处 PC 和 a0，不执行退出后的指令。
- 结束任务重复运行返回原结果，且完整现场不变。
- 独立 breakpoint 即使携带类似 `exit` 的寄存器值，也必须作为故障终止；保留故障 PC、整数寄存器及异常信息。随后新建正常任务仍能执行并退出，原故障现场不变。

成功时输出 `<架构>: task kernel tests passed`，仍须检查统一入口的总成功标记。测试不覆盖 ELF 加载、用户内存隔离、多任务调度或抢占；正常启动尚无用户程序，不创建任务。

任务对 syscall 完成失败的处理可用独立 debug QEMU 实例验证：在 `Task::run` 调用 `syscall::complete` 前暂停，将已捕获 syscall 现场的 PC 改为 `usize::MAX - 3`，再运行到保存终止状态处，检查任务成为 `Finished(Faulted)`，且注入后的完整现场保持不变。不恢复执行该边界地址；这项故障注入不属于上述顺序自检。

## 启用自检时的静态检查

特性分支的静态检查需显式传入功能：

```sh
make prepare-vendor
cargo clippy --config cargo/config.toml --locked --offline -p impl-os --target riscv64gc-unknown-none-elf --features ktest -- -D warnings
cargo clippy --config cargo/config.toml --locked --offline -p impl-os --target loongarch64-unknown-none --features ktest -- -D warnings
```
