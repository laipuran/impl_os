# 2026 OS 中西部与华南区域赛：工具链与测试集源码调研

> 调研日期：2026-10-26。本文以用户提供的官方镜像仓库、`oscomp` 测试仓库及镜像发布者的 Docker Hub 元数据为一手来源。**源码事实不等于 2026 区域赛最终规则**；下文单列工程建议与尚待确认事项。未下载镜像、工具链压缩包或测试二进制，未实际编译或运行内核。

> **后续实测更新**：本仓库随后已拉取上述固定 digest 的比赛镜像，确认其为 Ubuntu 24.04.4 / QEMU 10.0.2，并完成基架的双架构离线构建与启动。见[容器实测记录](testing/container-validation.md)。本文以下保留初次源码调研的证据边界；“本次未拉取/未运行”指最初的源码调研阶段，不是当前项目状态。

## 1. 版本锚点与最重要结论

| 对象 | 本次查到的固定版本 | 证据与含义 |
|---|---|---|
| `zhouzhouyi-hub/os-contest-image` HEAD | `7e6dc2a240d763993ca38ca9041d33a31a1b0fee`，2026-03-12 | [提交][image-commit]；仓库有 `Dockerfile` 和 `Dockerfile.orig`，不能混读 |
| `oscomp/testsuits-for-oskernel` 的 `pre-2025` | `8b58dd16d26d30f7c74d48d5832d870d3051b703`，2026-07-12 | [提交][tests-commit]；分支名虽然含 2025，但仍在更新，最新提交把默认镜像改为 `20250624` |
| 用户截图指定镜像 | `zhouzhouyi/os-contest:20260510` | [发布者 Docker Hub API][hub] 确认标签存在、状态 active、Linux/amd64，最后推送 2026-05-10；当日 digest 为 `sha256:85dec949df7cef41fd03d30c6ad69f952204540e18d2c62bced9d2e262fef12d` |

核心判断：

1. **截图镜像标签确实存在，但没有找到该标签与 Git 提交的一一对应证明。** Docker Hub 元数据只能确认镜像身份，不能证明里面的 GCC/Rust/QEMU 版本与仓库 HEAD 完全相同；镜像约 6.02 GB（API 的 `full_size`），本次没有拉取。[来源][hub]
2. 当前 `Dockerfile` 是 **Ubuntu 24.04 + 带 LoongArch 补丁的 QEMU 10.0.2**；`Dockerfile.orig` 是 **Ubuntu 22.04 + QEMU 9.2.1**。测试 README 的版本输出又主要是旧环境，不能拿 README 的旧版本冒充 20260510 镜像实测。[当前配方][dockerfile]、[旧配方][dockerfile-orig]、[测试 README][readme]
3. `pre-2025` 的构建意图是 **RISC-V / LoongArch × musl / glibc 四组合**，12 组测试；但 basic 自带无 libc 用户库，不是四份真正不同 libc 的 basic。[顶层 Makefile][makefile]、[子 Makefile][makefile-sub]、[basic CMake][basic-cmake]
4. **确有配方错位**：测试 Makefile 仍从 `/opt/riscv64--musl--bleeding-edge-2020.08-1/...` 拷贝 libc，而当前 Dockerfile 已换成 `riscv64-lp64d--musl--bleeding-edge-2024.02-1`。不能保证把当前配方构建出的环境直接用于该分支就能 `make` 成功。[来源][makefile]、[来源][dockerfile]
5. 本次证据不足以确认 2026 两区域的最终测试修订、计分权重、时间/内存/核数限制、提交要求。本文不提供“准确分数”，也不把历史分支当最终评测器。

### 获取边界与失效记录

- 用户给的 `https://github.com/oscomp/testsuits-for-oskernel/blob/pre-2025/` 是不带文件名的 blob 形式；研究对象应使用 [tree/pre-2025](https://github.com/oscomp/testsuits-for-oskernel/tree/pre-2025) 或以下固定提交链接。GitHub API 成功解析了该分支。
- 镜像仓库固定提交的 `README.md` raw URL 返回 **404**；不能据此认定整个仓库失效。读取到的是 Dockerfile、依赖清单及目录树。
- 两个仓库的浅克隆均在 120 秒超时；随后改用 GitHub API 和 raw 文件读取，没有成功克隆的假设，也没有把大体积上游内容导入本仓库。
- 测试 README 还链接旧 GitLab Dockerfile 地址；本文未把该外链当作已经核实的 2026 配方。

## 2. 交叉工具链：命令、安装路径、版本证据

以下“当前配方”均指固定提交的 [`Dockerfile`][dockerfile]；“历史输出”指固定测试提交的 [`README.md`][readme]。**包名中的日期/版本与实际 `--version` 是不同证据等级。**

| 用途/命令 | 当前配方中的路径/来源 | 可确认版本及限制 |
|---|---|---|
| RISC-V GNU/Linux `riscv64-linux-gnu-gcc`、`g++` | apt 安装 `gcc-riscv64-linux-gnu`、`g++-riscv64-linux-gnu`、`binutils-riscv64-linux-gnu`；可执行通常在 `/usr/bin`，最终以 `command -v` 为准 | 未锁 apt 包版本；历史 README 为 Ubuntu 22.04 的 GCC **11.4.0**，不能外推到 Ubuntu 24.04 |
| RISC-V bare-metal `riscv64-unknown-elf-gcc` | COPY `/opt/kendryte-toolchain`，其 `bin` 追加 PATH；另 apt 安装同名 bare-metal GCC 软件包 | 仓库 Kendryte 目录与历史输出指向 **8.2.0**；存在 PATH 同名覆盖风险，不能只凭安装目录断言实际命中的版本 |
| RISC-V Buildroot musl `riscv64-buildroot-linux-musl-gcc`（旧环境另有 `riscv64-linux-gcc`） | 当前配方解包至 `/opt/riscv64-lp64d--musl--bleeding-edge-2024.02-1`，`bin` 追加 PATH | 当前包日期 **2024.02-1**，配方没有打印 GCC 精确版本；旧包 2020.08-1 的历史输出为 Buildroot **2020.08-14-ge5a2a90 / GCC 10.2.0** |
| RISC-V musl `riscv64-linux-musl-gcc` | `/opt/riscv64-linux-musl-cross/bin` 前置 PATH | 压缩包未锁可读版本；历史 README 为 **11.2.1 20211120**，不能确证当前镜像仍是这个版本 |
| LoongArch GNU/Linux `loongarch64-linux-gnu-gcc` | `/opt/gcc-13.2.0-loongarch64-linux-gnu/bin` 前置；也保留 `/opt/toolchain-loongarch64-linux-gnu-gcc8-host-x86_64-2022-07-18/bin` | 新包名和历史输出共同指向 **13.2.0**；旧工具链包名含 **gcc8 / 2022-07-18**，不能细化成未见证据的小版本 |
| LoongArch musl `loongarch64-linux-musl-gcc` | `/opt/loongarch64-linux-musl-cross/bin` 前置 PATH | 历史 README 为 **13.2.0**；当前包名本身没有版本号 |
| QEMU | `/opt/qemu-bin-10.0.2/bin` 最后前置 PATH | 从 `qemu-10.0.2-la-patchJuly142025.tar.xz` 构建，启用 slirp，target-list 为 `loongarch64,riscv64,aarch64,x86_64` 的 softmmu；补丁内容未展开核查 |

配方里出现 QEMU 7.0.0、2K1000 压缩包，并不意味着最终运行它们：最终阶段实际复制并前置 PATH 的是 QEMU 10.0.2。旧 `Dockerfile.orig` 则复制 `/opt/qemu-bin-9.2.1`，使用旧 Buildroot 2020.08-1 包。[当前][dockerfile]、[旧版][dockerfile-orig]

### 2.1 Rust toolchain / target / component

以下是 Dockerfile 的显式安装集合，不是 20260510 容器实测；宿主 target `x86_64-unknown-linux-gnu` 也可能随 rustup 默认组件安装，不应与交叉 target 混为一谈。[逐项来源][dockerfile]

| nightly | 显式交叉 target | 显式额外 component |
|---|---|---|
| **2025-01-18（默认）** | `riscv64imac-unknown-none-elf`、`riscv64gc-unknown-none-elf`、`loongarch64-unknown-linux-gnu`、`loongarch64-unknown-none` | `llvm-tools-preview` |
| 2024-02-03、2024-05-01 | `riscv64imac-unknown-none-elf`、`riscv64gc-unknown-none-elf`、`loongarch64-unknown-linux-gnu` | `llvm-tools-preview` |
| 2024-08-01 | 上述两个 RISC-V target；`loongarch64-unknown-none`、`loongarch64-unknown-linux-gnu`、`x86_64-unknown-none`、`aarch64-unknown-none-softfloat` | `llvm-tools-preview` |
| 2025-02-01 | 上述两个 RISC-V target；`loongarch64-unknown-none`、`x86_64-unknown-linux-gnu`、`x86_64-unknown-none` | `clippy`、`rust-src`、`rustfmt`、`llvm-tools-preview` |
| 2025-05-20 | 上述两个 RISC-V target；`loongarch64-unknown-none`、`loongarch64-unknown-none-softfloat` | `rust-src`、`llvm-tools-preview` |

- rustup 安装在 root 用户环境，PATH 包含 `/root/.cargo/bin`；默认目录可用 `rustup show home` 复核，不能把惯例路径代替镜像实测。[来源][dockerfile]
- `cargo-binutils` 在多个 nightly 下反复 `cargo install`，不是为每个 nightly 自动维护互不干扰的独立命令集；除第一次 `--locked` 外，多数没有锁版本。2025-05-20 没有单列 `cargo install cargo-binutils`。[来源][dockerfile]
- Rust target 是内核构建选项，不是测试集 libc 矩阵：测试集主体使用 C 交叉编译器。内核选 `riscv64gc-unknown-none-elf` 不代表不需要 Linux 用户态 ABI/浮点上下文支持；basic 的 RISC-V 编译参数明确含 `-march=rv64imafdc -mabi=lp64d`。[来源][basic-cmake]

### 2.2 预装依赖及离线构建风险

当前配方安装 `build-essential`、make、cmake、autotools/automake/texinfo、Python3/pip、gdb-multiarch、musl/musl-tools/musl-dev、`libclang-19-dev`、libguestfs-tools、qemu-utils、linux-image-generic、fusefat，以及 QEMU 构建需要的 ninja、pkg-config、glib/pixman/slirp 开发包等。**安装 libclang-19-dev 不等于明确安装并锁定 clang/LLD 整套工具链。** Python 依赖有 tomli、pytz、Cython、jwt、jinja2、requests，均未锁精确版本。[来源][dockerfile]

`/oscomp-dependencies-2023/Cargo.toml` 包名是 `oscomp-dependencies-old`，edition 2021，声明：`rustsbi="0.3.2"`、`futures-lite="1.12.0"`、`crossbeam-queue="0.3.8"`、`heapless="0.7.16"`、`async-task="4.4.0"`、`bitflags="2.0.2"`、`log="0.4"`、`num_enum={version="0.5.11",default-features=false}`。Dockerfile 执行 `cargo build` 预热缓存，并将 crates.io 替换为阿里云 sparse 镜像。[清单][cargo-deps]、[配方][dockerfile]

**建议**：这些是 Cargo 兼容版本范围，不是精确锁定版本，也不覆盖任意参赛项目依赖。提交前用自己的 `Cargo.lock`、`cargo build --locked --offline` 验证；不能因为镜像有预热项目就宣称所有 crate 离线可用。

## 3. 测试目录与构建产物

固定提交 [README][readme] 列出 12 组：basic、busybox、lua、libctest、iozone、unixbench、iperf、libcbench、lmbench、netperf、cyclictest、LTP。源目录名、脚本组名与可执行文件名并不总一致。

| 源目录 | 运行入口（位于每个 libc 目录） | 源码明确覆盖的内容 |
|---|---|---|
| `basic/user` | `basic_testcode.sh` → `basic/run-all.sh` | 32 个基础程序，详见下一节；basic 自带 ulib，不链接 musl/glibc |
| `busybox`、`config` | `busybox_testcode.sh` + `busybox_cmd.txt` | shell、命令、文件、管道与后台作业 |
| `lua` | `lua_testcode.sh` → `test.sh` | date/file_io/max_min/random/remove/round_num/sin30/sort/strings，9 个脚本 |
| `libc-test` | `libctest_testcode.sh` → `run-static.sh` / `run-dynamic.sh` | `static.txt` / `dynamic.txt` 控制打包列表；聚合为 `entry-static.exe` / `entry-dynamic.exe`，不是每个条目一个独立 ELF |
| `iozone` | `iozone_testcode.sh` | 自动读写、4 worker 吞吐、随机/逆向/跨距读、stdio、pread/pwrite、向量定位 I/O |
| `UnixBench` | `unixbench_testcode.sh` | 算术、syscall、context、pipe、spawn、exec、文件读写复制、shell 并发 |
| `iperf` | `iperf_testcode.sh`，程序为 `iperf3` | 127.0.0.1:5001；TCP/UDP、5 路 parallel、reverse，共 6 种组合 |
| `libc-bench` | `libcbench_testcode.sh` → `libc-bench` | libc 性能基准 |
| `lmbench_src` | `lmbench_testcode.sh` → `lmbench_all` | syscall/select/signal/pipe/process、文件页错误与 mmap、文件系统/带宽/context |
| `netperf` | `netperf_testcode.sh` | 解包 netperf 2.7.0；127.0.0.1:12865，UDP_STREAM/TCP_STREAM/UDP_RR/TCP_RR/TCP_CRR |
| `rt-tests-2.7` | `cyclictest_testcode.sh` | cyclictest 1/8 线程，无压力/有 hackbench 压力；优先级 99、1ms 间隔、每轮 1s |
| `ltp-full-20240524` | `ltp_testcode.sh` | 遍历 `ltp/testcases/bin/*` 的普通文件，逐个运行；不能把全部 LTP 上游测试数量当本赛题数量 |

表中构建关系来源：[Makefile.sub][makefile-sub]；运行参数来源：[固定 scripts 目录][scripts]；libctest 打包来源：[libc-test/Makefile][libctest-make]；LTP 构建来源：[build-oscomp.sh][ltp-build]。

### 3.1 四组合与动态加载器

顶层 Makefile 的产物是：

```text
sdcard/riscv/{musl,glibc}/...     -> sdcard-rv.img.xz
sdcard/loongarch/{musl,glibc}/...  -> sdcard-la.img.xz
镜像内部根目录：/musl、/glibc
```

每个架构的 EXT4 镜像无分区表；Makefile 用 `dd count=4096 bs=1M` 建 4 GiB 原始镜像，挂载后 `cp -rL`，再 xz 压缩。README 所说“磁盘根目录测试脚本”是概括，按当前 Makefile 实际脚本在 `/musl` 和 `/glibc` 下。Makefile 用 sed 将组名改成 `basic-musl` / `basic-glibc` 等。[来源][makefile]、[README][readme]

| 组合 | 主编译前缀 | 复制库的位置 |
|---|---|---|
| RISC-V musl | `riscv64-buildroot-linux-musl-` | 旧 Buildroot sysroot 的 `lib/libc.so` |
| RISC-V glibc | `riscv64-linux-gnu-` | `/usr/riscv64-linux-gnu/lib`：libc.so.6、libm.so.6、`ld-linux-riscv64-lp64d.so.1` 等 |
| LoongArch musl | `loongarch64-linux-musl-` | `/opt/loongarch64-linux-musl-cross/loongarch64-linux-musl/lib/libc.so` |
| LoongArch glibc | `loongarch64-linux-gnu-` | `/opt/gcc-13.2.0-loongarch64-linux-gnu/sysroot/usr/lib64`：libc.so.6、libm.so.6、`ld-linux-loongarch-lp64d.so.1` |

以上是复制路径，**不是已经检查过每个 ELF 的 PT_INTERP**。实际装载器路径、库搜索路径、架构/ABI 必须用 `readelf -l/-h/-d` 验证。[来源][makefile]

特殊情况不能忽略：basic 的 CMake 硬编码 `${ARCH}-linux-gnu-`、`-nostdlib` 和自带 ulib；iperf 按 DESTDIR 判断架构/libc；LTP 的 RISC-V musl 明确用 **`riscv64-linux-musl-gcc`**，不是主构建的 Buildroot 编译器。所以“一个 PREFIX 编译所有测试”并不准确。[basic][basic-cmake]、[iperf][iperf-build]、[LTP][ltp-build]

### 3.2 构建方式与资源/兼容性注意事项

上游文档方法为 `make docker` 后 `make`；实际 `all → sdcard → build-all → build-rv/build-la`，每种 libc 之间执行 clean。Docker target 使用 `--privileged`、绑定当前目录到 `/code`，便于 mount EXT4；应只在可信环境运行。[README][readme]、[Makefile][makefile]

```bash
# 以下为复核/复现命令，不是本次已经执行成功的构建记录。
git clone --depth 1 --branch pre-2025 https://github.com/oscomp/testsuits-for-oskernel.git
cd testsuits-for-oskernel
git rev-parse HEAD
make docker                         # 上游默认 20250624，而非截图 20260510
# 容器内，先核对工具链与路径，再构建：
make build-all                      # 只编译并整理目录，不做 4 GiB 镜像
make                                # 再完整重编译并制作两个镜像；需空间/挂载权限
```

如需对截图环境复现，可在宿主执行 `make docker DOCKER=zhouzhouyi/os-contest:20260510`，但这会下载大镜像，本次未执行。应先检查旧 Buildroot 路径是否存在；不能靠随意软链接掩盖 ABI/libc 不一致。两个架构的构建共享源目录且会 clean，**不要直接 `make -j build-all`**；这是根据构建配方给出的工程建议。[来源][makefile]

## 4. 运行、输出协议与脚本陷阱

### 4.1 历史评测约定（不是已经确认的 2026 规则）

固定 README 要求项目根目录 `make all` 生成 ELF `kernel-rv`、`kernel-la`；可选额外磁盘；内核启动后主动运行测试，串行运行各组、向屏幕输出、最后主动关机。其 RISC-V QEMU 示例为 virt、默认 BIOS、virtio-mmio block/net；LoongArch 示例使用 virtio-blk-pci/net-pci。内存和 SMP 是 `{mem}` / `{smp}` 占位符，没有给出可据以确认的 2026 固定数值。[来源][readme]

**注意历史文档内部不一致**：LoongArch 示例将 PCI 设备同时指定 `bus=virtio-mmio-bus.0/.1`；附加盘正文称 `disk.img`，LoongArch 示例写 `disk-la.img`。这些示例不能不经 `-device ... ,help` 和实际 QEMU 验证就作为可靠启动命令。[原始示例][readme]

推荐在内核的用户态 init/runner 中进入 `/musl` 或 `/glibc`，设置 PATH、准备 `/tmp`、`/var/tmp`、所需设备/伪文件系统及 shell 路径，再执行脚本。因为脚本大量使用 `./busybox` 和相对路径，**从磁盘根目录直接调用 `/musl/xxx_testcode.sh` 而不切 cwd 很可能失败**。[脚本证据][scripts]（这里是运行建议，不是官方 init 规范。）

“组串行”不等于禁用测试内部并发：iperf/netperf 服务端、iozone worker、cyclictest/hackbench 和 shell pipeline 都依赖组内多进程/多线程。[脚本证据][scripts]

### 4.2 必须保留的输出

```text
#### OS COMP TEST GROUP START basic-musl ####
...真实测试输出...
#### OS COMP TEST GROUP END basic-musl ####
```

源脚本初始组名为 `basic` 等，构建时才追加 `-musl` / `-glibc`。README 明确提示：若不直接运行脚本，也须保留组前后提示。不能只打印成功标记代替实际测试。[Makefile][makefile]、[README][readme]

- **basic**：`Testing <name> :`；用户库宏生成 `========== START <name> ==========` / `END`。[run-all][basic-run]、[stdio.h][basic-stdio]
- **busybox**：`testcase busybox <命令> success|fail`；命令 `false` 的非零返回值被有意视为成功。[脚本][busybox-script]
- **lua**：`testcase lua <文件> success|fail`，按 Lua 进程返回值判断。[脚本][lua-script]
- **libctest**：runtest 打印 START/END、成功 `Pass!`、失败 `FAIL ... [status ...]` / `[timed out]` / `[signal ...]` 等；默认每项超时 5 秒，依赖 fork/exec、SIGCHLD 屏蔽、sigtimedwait、kill、waitpid、setrlimit。**runtest 的 main 最后无条件 `return 1`，不能仅按 runner 退出码判定每项失败。**[源码][runtest]
- **UnixBench**：原始 `COUNT|...|` 经 grep/awk 转换成 `Unixbench ... test(lps/KBps/MFLOPS/lpm): ...`；保留 pipeline、定时和真实数值输出。[脚本][unixbench-script]
- **iperf/netperf/cyclictest**：各子项使用 `====== ... begin ======` / `====== ... end: success|fail ======`，按相应命令退出码判断。[脚本目录][scripts]
- **LTP**：脚本每个普通文件打印 `RUN LTP CASE <name>`，运行后**无论返回码是否为 0 都打印** `FAIL LTP CASE <name> : <ret>`。单独检索字符串 `FAIL` 会误判；需结合返回值与该用例原始输出解释，2026 正式解析器如何处理尚未知。[脚本][ltp-script]

### 4.3 不要直接依赖 `scripts/test_all.sh`

该脚本不是权威完备清单：开头跑 `time-test`、缺 basic、LTP 被注释、直接运行 libc-bench 而不加组包装，调用 `cyclic_testcode.sh` 但实际安装名是 `cyclictest_testcode.sh`；顶层 `Makefile.sub` 也未将它作为总入口复制。应以各组实际安装脚本逐一运行。[test_all.sh][test-all]、[安装配方][makefile-sub]

脚本 shebang 也不统一：有 `/bin/bash`、`/busybox sh`、`/bin/busybox sh`，另有无 shebang 的脚本。建议先验证 BusyBox ash 能力（`[[ ]]`、eval、后台进程、管道等）并显式用解释器运行，或正确提供解释器路径，不要把 shebang 缺失/路径缺失误诊为 ELF 装载失败。[脚本目录][scripts]

## 5. 内核能力与系统调用覆盖

### 5.1 basic：可逐项追踪的最小闭环

`basic/run-all.sh` 列出 32 个实际运行名称：

```text
brk chdir clone close dup2 dup execve exit fork fstat getcwd getdents
getpid getppid gettimeofday mkdir_ mmap mount munmap openat open pipe read
sleep times umount uname unlink wait waitpid write yield
```

这是程序名，不是 Linux syscall 名的一一映射；例如 dup2、open、sleep、fork、wait 等可能由封装转换到 dup3/openat/nanosleep/clone/wait4。[测试顺序][basic-run]、[封装源码][basic-syscall]

对应基础能力：用户态陷入/返回；ELF 装载与 argv/env；进程地址空间与父子关系；阻塞等待/退出回收；页映射、brk/mmap/munmap；文件与目录、fd 共享/复制、pipe、挂载；时钟、睡眠、uname。`oscomp_syscalls.md` 提供 RISC-V ABI（a7 号、a0–a5 参数、a0 返回），但它不是完整 2026 syscall 清单，也不能照搬其中 libc 风格返回值描述为原始 Linux ABI。[说明][syscalls]、[封装源码][basic-syscall]

### 5.2 扩展能力矩阵（依据测试内容推导，非穷尽 syscall 清单）

| 测试族 | 开发中应重点核对的能力 | 直接源码依据 |
|---|---|---|
| BusyBox / Lua | cwd、目录遍历、stat、重命名/删除、重定向、pipe、shell exec/wait；时钟、睡眠；ps/free/df/dmesg 等还涉及伪文件系统或对应查询接口 | [命令列表][busybox-cmd]、[Lua 脚本][lua-dir] |
| libctest 静态 | ELF 初始栈、auxv、TLS、正确 errno/边界检查、线程/同步、时钟/信号、基本 socket；纯字符串/数学项也先依赖 runner 正常工作 | [static.txt][static-list]、[runtest][runtest] |
| libctest 动态 / glibc | PT_INTERP、共享库文件映射、mmap/mprotect、TLS、dlopen；内核负责装载器需要的 ABI/映射，不是替用户态动态链接器实现全部符号重定位 | [dynamic.txt][dynamic-list]、[链接配方][libctest-make] |
| pthread / cyclictest | clone、TLS、futex、线程退出、robust-list、信号、affinity、调度策略/优先级、clock_nanosleep/定时器；不能只实现“返回成功”的调度接口 | [dynamic.txt][dynamic-list]、[cyclictest 脚本][cyclictest-script] |
| Iozone / 文件基准 | seek、定位/向量 I/O、同步、mmap 文件页、页错误与回写、多进程一致性 | [iozone 脚本][iozone-script]、[lmbench 脚本][lmbench-script] |
| UnixBench / lmbench | fork/exec/wait、pipe/select、signal handler 与 sigreturn、上下文切换、公平调度及真实计时 | [UnixBench][unixbench-script]、[lmbench][lmbench-script] |
| iperf / netperf | 真实 loopback TCP/UDP、bind/listen/accept/connect、send/recv、socket 选项、阻塞唤醒、多连接；不仅是外部网卡驱动 | [iperf][iperf-script]、[netperf][netperf-script] |
| LTP | 广泛 Linux 兼容性：错误码、权限/资源限制、信号/IPC/文件系统/内存语义等；需按实际安装文件和依赖逐项收敛 | [构建][ltp-build]、[运行器][ltp-script] |

表内 syscall 名是能力排查方向，不能声称每项在四种构建里都必然使用同一 syscall；libc、架构、构建配置会改变调用路径。尤其 glibc 与 musl 的线程、TLS、动态加载路径不能视为等价。

LTP 构建脚本注释提到排除 `fmtmsg rt_sigtimedwait rt_tgsigqueueinfo timer_create`，并记录 sysinfo 头文件修改；这不是“所有信号/定时器均不考”的证据。实际构建目录与最终安装结果仍需检查。[来源][ltp-build]

## 6. 推荐实施优先顺序（工程建议，不是评分排名）

1. **环境冻结与启动闭环**：记录镜像 digest、所有编译器实际路径/版本；两架构启动、串口、关机、virtio block、EXT4 读盘；先跑一个真实静态 ELF。
2. **basic 全链条**：按现有 32 项逐项检查返回值和副作用，补齐 VFS、pipe、进程/内存/等待。尽早在两架构保持相同回归集。
3. **BusyBox + Lua**：打通用户态 shell/runner、cwd/PATH/解释器、重定向/管道和可写目录；这是大量后续脚本的基础设施，不宜拖到最后。
4. **libctest runner → 静态子集 → 动态/glibc**：先解决 SIGCHLD/超时机制，避免每个“纯 libc”用例都被运行器阻断；再完善 auxv、TLS、futex、信号帧及动态加载器。
5. **文件与进程基准**：Iozone、libcbench、UnixBench、lmbench；先正确后性能，重点缓存/页错误/调度/时钟，防止空操作实现产生虚假高吞吐。
6. **loopback 网络 + 线程实时负载**：iperf/netperf、cyclictest；如已有网络栈可提前网络组，并行线程/信号基础成熟后再上压力项。
7. **LTP 按依赖分层**：从已支持 syscall 的确定性用例向外扩展，单独记录 unsupported、timeout、语义失败、环境缺失；不要一次全跑后只数 `FAIL`。

依据是上述脚本依赖关系和能力成本，不是未知的 2026 权重。README 仅对该历史评测说明“测试点执行顺序与评分无关”；不能外推为 2026 排名/同分规则。[历史说明][readme]

## 7. 可验证命令与正式比赛前确认清单

### 7.1 不下载镜像即可核对来源

```bash
curl -fsSL https://api.github.com/repos/zhouzhouyi-hub/os-contest-image/commits/HEAD
curl -fsSL https://api.github.com/repos/oscomp/testsuits-for-oskernel/commits/pre-2025
curl -fsSL https://hub.docker.com/v2/repositories/zhouzhouyi/os-contest/tags/20260510
curl -fsSL https://raw.githubusercontent.com/zhouzhouyi-hub/os-contest-image/7e6dc2a240d763993ca38ca9041d33a31a1b0fee/Dockerfile
curl -fsSL https://raw.githubusercontent.com/oscomp/testsuits-for-oskernel/8b58dd16d26d30f7c74d48d5832d870d3051b703/Makefile
```

本次已通过 HTTP 工具核查这些接口的内容；上面是供人复查的等价 curl 命令。HEAD、分支和 tag 可变，复现应保存响应并使用固定提交/digest。

### 7.2 在已经具备指定镜像的环境里核查（本次未执行）

```bash
docker image inspect zhouzhouyi/os-contest:20260510 --format '{{json .RepoDigests}}'
docker run --rm --entrypoint bash zhouzhouyi/os-contest:20260510 -lc '
  cat /etc/os-release
  echo "$PATH"
  for cc in riscv64-linux-gnu-gcc riscv64-unknown-elf-gcc riscv64-buildroot-linux-musl-gcc riscv64-linux-musl-gcc loongarch64-linux-gnu-gcc loongarch64-linux-musl-gcc; do
    type -a "$cc"
    "$cc" --version
    "$cc" -dumpmachine
    "$cc" -print-sysroot
  done
  qemu-system-riscv64 --version
  qemu-system-loongarch64 --version
  rustup show
  rustc -Vv
  cargo -V
  rustup target list --installed --toolchain nightly-2025-01-18
  rustup component list --installed --toolchain nightly-2025-01-18
  ls -ld /opt/* /usr/riscv64-linux-gnu
'

# 构建产物生成后，检查而不执行目标二进制：
readelf -h sdcard/riscv/glibc/entry-dynamic.exe
readelf -l sdcard/riscv/glibc/entry-dynamic.exe
readelf -d sdcard/riscv/glibc/entry-dynamic.exe
find sdcard -name '*_testcode.sh' -print
grep -R 'OS COMP TEST GROUP' sdcard --include='*_testcode.sh'
```

如果本地没有镜像，`docker run` 可能自动 pull；执行前确认网络/空间策略。需要完全禁止下载时加 `--pull=never`。

### 7.3 必须向 2026 官方规则/评测平台确认

- 两区域是否使用相同 **镜像 digest、测试 commit、磁盘镜像哈希**，20260510 是编译环境、运行环境还是二者；是否有后续热修订。
- 最终架构/libc 组合、必测/选测项、测试点选择、超时/内存/SMP/磁盘写入限制、评分解析器与权重。
- `make all`、内核文件名/格式、附加磁盘名、QEMU 完整启动命令、BIOS/固件版本、是否必须主动关机。
- glibc/动态链接支持范围、网络测试配置、实时调度权限/能力、LTP 实际安装清单，以及已知脚本异常是否已修复。

在获得上述确认前，最可靠的定位是：**本文是可追溯的工具链/历史测试源码地图和备赛路线，不是 2026 最终规则汇编。**

[image-commit]: https://github.com/zhouzhouyi-hub/os-contest-image/commit/7e6dc2a240d763993ca38ca9041d33a31a1b0fee
[tests-commit]: https://github.com/oscomp/testsuits-for-oskernel/commit/8b58dd16d26d30f7c74d48d5832d870d3051b703
[hub]: https://hub.docker.com/v2/repositories/zhouzhouyi/os-contest/tags/20260510
[dockerfile]: https://github.com/zhouzhouyi-hub/os-contest-image/blob/7e6dc2a240d763993ca38ca9041d33a31a1b0fee/Dockerfile
[dockerfile-orig]: https://github.com/zhouzhouyi-hub/os-contest-image/blob/7e6dc2a240d763993ca38ca9041d33a31a1b0fee/Dockerfile.orig
[cargo-deps]: https://github.com/zhouzhouyi-hub/os-contest-image/blob/7e6dc2a240d763993ca38ca9041d33a31a1b0fee/oscomp-dependencies-2023/Cargo.toml
[readme]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/README.md
[makefile]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/Makefile
[makefile-sub]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/Makefile.sub
[scripts]: https://github.com/oscomp/testsuits-for-oskernel/tree/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts
[basic-cmake]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/basic/user/CMakeLists.txt
[basic-run]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/basic/user/src/oscomp/run-all.sh
[basic-stdio]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/basic/user/include/stdio.h
[basic-syscall]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/basic/user/lib/syscall.c
[syscalls]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/oscomp_syscalls.md
[iperf-build]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/iperf/build.sh
[ltp-build]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/ltp-full-20240524/build-oscomp.sh
[libctest-make]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/libc-test/Makefile
[runtest]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/libc-test/src/common/runtest.c
[static-list]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/libc-test/static.txt
[dynamic-list]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/libc-test/dynamic.txt
[busybox-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/busybox/busybox_testcode.sh
[busybox-cmd]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/busybox/busybox_cmd.txt
[lua-dir]: https://github.com/oscomp/testsuits-for-oskernel/tree/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/lua
[lua-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/lua/test.sh
[unixbench-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/unixbench/unixbench_testcode.sh
[lmbench-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/lmbench/lmbench_testcode.sh
[iozone-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/iozone/iozone_testcode.sh
[cyclictest-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/cyclictest/cyclictest_testcode.sh
[iperf-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/iperf/iperf_testcode.sh
[netperf-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/netperf/netperf_testcode.sh
[ltp-script]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/ltp/ltp_testcode.sh
[test-all]: https://github.com/oscomp/testsuits-for-oskernel/blob/8b58dd16d26d30f7c74d48d5832d870d3051b703/scripts/test_all.sh
