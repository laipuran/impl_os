# Make 负责内核产物；Just 直接运行开发命令。
set positional-arguments

contest_image := `sed -n 's/^ARG CONTEST_IMAGE=//p' .devcontainer/Dockerfile`
dev_image := "impl-os-dev:20260510"
# Export before the shell starts so MSYS cannot rewrite container paths.
export MSYS_NO_PATHCONV := if os() == "windows" { "1" } else { env_var_or_default("MSYS_NO_PATHCONV", "") }
export MSYS2_ARG_CONV_EXCL := if os() == "windows" { "*" } else { env_var_or_default("MSYS2_ARG_CONV_EXCL", "") }
# Only Linux bind mounts require host IDs; Git Bash IDs are not Linux IDs.
dev_uid := if os() == "linux" { `id -u` } else { "1000" }
dev_gid := if os() == "linux" { `id -g` } else { "1000" }
toolchain := `sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml`
memory := env_var_or_default("MEM", "1G")
cpus := env_var_or_default("SMP", "1")
rv := "qemu-system-riscv64 -machine virt -bios default -nographic -no-reboot -rtc base=utc -device virtio-net-device,netdev=net,bus=virtio-mmio-bus.2 -netdev user,id=net"
la := "qemu-system-loongarch64 -machine virt -nographic -no-reboot -rtc base=utc -device virtio-net-pci,netdev=net -netdev user,id=net"

# 列出任务；不构建或安装任何内容。
default:
    @just --list

# [容器] 显式联网配置 Rust；不作为构建依赖。
setup:
    rustup toolchain install "{{toolchain}}" --no-self-update --profile minimal --component rustfmt,clippy,llvm-tools-preview,rust-analyzer,rust-src --target riscv64gc-unknown-none-elf,loongarch64-unknown-none

# [容器] 只读诊断；缺少工具时失败，不自动安装。
doctor:
    #!/bin/sh
    set -eu
    export RUSTUP_AUTO_INSTALL=0
    just --version
    make --version
    rustup run "{{toolchain}}" rustc --version
    rustup run "{{toolchain}}" cargo --version
    rustup target list --installed --toolchain "{{toolchain}}" | grep -Fx riscv64gc-unknown-none-elf
    rustup target list --installed --toolchain "{{toolchain}}" | grep -Fx loongarch64-unknown-none
    rustup run "{{toolchain}}" cargo fmt --version
    rustup run "{{toolchain}}" cargo clippy --version
    rustup run "{{toolchain}}" rust-analyzer --version
    qemu-system-riscv64 --version
    qemu-system-loongarch64 --version
    gdb-multiarch --version

# [容器] 通过 Make 构建 all/rv/la；可选模式：release/debug。
build arch="all" mode="release":
    #!/bin/sh
    set -eu
    case "$1" in
        all) target=all ;;
        rv|la) target="kernel-$1" ;;
        *) echo 'architecture must be all, rv or la' >&2; exit 2 ;;
    esac
    exec make "$target" MODE="$2"

# [容器] 通过 Make 仅删除生成的构建产物。
clean:
    make clean

# [容器] 格式化 Rust 源码（会修改文件）。
fmt:
    cargo fmt --all

# [容器] 检查格式，不修改文件。
fmt-check:
    cargo fmt --all -- --check

# [容器] 对两个裸机目标运行 Clippy；不导出 ELF 文件。
lint:
    cargo clippy --locked --offline -p impl-os --target riscv64gc-unknown-none-elf -- -D warnings
    cargo clippy --locked --offline -p impl-os --target loongarch64-unknown-none -- -D warnings

# [容器] 仅进行静态检查；不执行内核测试或启动验证。
check: fmt-check lint

# [容器] 构建 release 模式的 rv/la 并启动；参数使用 QEMU 原生选项。
run arch *args: (build arch)
    @just _qemu "$@"

# [容器] 构建 debug 模式的 rv/la，暂停 QEMU 并在 :1234 等待 GDB。
debug arch *args: (build arch "debug")
    @just _qemu "$@" -S -gdb tcp::1234

_qemu arch *args:
    #!/bin/sh
    set -eu
    arch=$1
    shift
    case "$arch" in
        rv) exec {{rv}} -kernel kernel-rv -m "{{memory}}" -smp "{{cpus}}" "$@" ;;
        la) exec {{la}} -kernel kernel-la -m "{{memory}}" -smp "{{cpus}}" "$@" ;;
        *) echo 'run/debug architecture must be rv or la' >&2; exit 2 ;;
    esac

# [宿主] 显式下载固定版本的官方基础镜像。
image-pull:
    docker pull --platform linux/amd64 "{{contest_image}}"

# [宿主] 在镜像构建时缓存 Linux 宿主 UID/GID 和工具，不在容器启动时配置。
image-build:
    docker build --platform linux/amd64 --build-arg "DEV_UID={{dev_uid}}" --build-arg "DEV_GID={{dev_gid}}" --tag "{{dev_image}}" --file .devcontainer/Dockerfile .

# [宿主] 使用镜像的 dev 用户运行；不迁移文件属主，也不自动拉取镜像。
container *command:
    #!/bin/sh
    set -eu
    if [ "$#" -eq 0 ]; then set -- bash; fi
    set -- "{{dev_image}}" "$@"
    if [ -t 0 ] && [ -t 1 ]; then set -- -it "$@"; fi
    exec docker run --rm --pull=never --platform linux/amd64 --mount "type=bind,source={{justfile_directory()}},target=/workspace/impl_os" --workdir /workspace/impl_os "$@"

# [宿主] 在原始官方镜像中离线进行全新 Cargo 构建；不启动或测试。
judge-build:
    #!/bin/sh
    set -eu
    docker run --rm --pull=never --platform linux/amd64 --network none --env RUSTUP_AUTO_INSTALL=0 --env "LOCAL_UID={{dev_uid}}" --env "LOCAL_GID={{dev_gid}}" --mount "type=bind,source={{justfile_directory()}},target=/workspace/impl_os" --workdir /workspace/impl_os --entrypoint /bin/sh "{{contest_image}}" -c '
        # Keep the pristine image and its root-only toolchain. Only exported
        # ELF files touch the workspace; repair their ownership even on failure.
        trap '\''for file in kernel-rv kernel-la; do
            if [ -f "$file" ] && [ ! -L "$file" ]; then
                chown "$LOCAL_UID:$LOCAL_GID" "$file"
            fi
        done'\'' EXIT
        make all TARGET_DIR=/tmp/impl-os-judge-target
    '
