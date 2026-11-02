# Make owns kernel artifacts; Just directly runs development commands.
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

# List tasks; does not build or install anything.
default:
    @just --list

# [Container] Explicit online Rust setup; never a build dependency.
setup:
    rustup toolchain install "{{toolchain}}" --no-self-update --profile minimal --component rustfmt,clippy,llvm-tools-preview,rust-analyzer,rust-src --target riscv64gc-unknown-none-elf,loongarch64-unknown-none

# [Container] Read-only diagnostics; fail on missing tools, never auto-install.
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

# [Container] Build all/rv/la via Make; optional mode: release/debug.
build arch="all" mode="release":
    #!/bin/sh
    set -eu
    case "$1" in
        all) target=all ;;
        rv|la) target="kernel-$1" ;;
        *) echo 'architecture must be all, rv or la' >&2; exit 2 ;;
    esac
    exec make "$target" MODE="$2"

# [Container] Remove generated build artifacts only, via Make.
clean:
    make clean

# [Container] Format Rust sources (modifies files).
fmt:
    cargo fmt --all

# [Container] Check formatting without modifying files.
fmt-check:
    cargo fmt --all -- --check

# [Container] Clippy on both bare-metal targets; no exported ELF files.
lint:
    cargo clippy --locked --offline -p impl-os --target riscv64gc-unknown-none-elf -- -D warnings
    cargo clippy --locked --offline -p impl-os --target loongarch64-unknown-none -- -D warnings

# [Container] Static checks only; not kernel tests or boot verification.
check: fmt-check lint

# [Container] Build release rv/la and boot; args are native QEMU options.
run arch *args: (build arch)
    @just _qemu "$@"

# [Container] Build debug rv/la and pause QEMU for GDB on :1234.
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

# [Host] Explicitly download the pinned official base image.
image-pull:
    docker pull --platform linux/amd64 "{{contest_image}}"

# [Host] Cache Linux host IDs and tools in the image, never at container startup.
image-build:
    docker build --platform linux/amd64 --build-arg "DEV_UID={{dev_uid}}" --build-arg "DEV_GID={{dev_gid}}" --tag "{{dev_image}}" --file .devcontainer/Dockerfile .

# [Host] Run as the image's dev user; never migrate ownership or auto-pull.
container *command:
    #!/bin/sh
    set -eu
    if [ "$#" -eq 0 ]; then set -- bash; fi
    set -- "{{dev_image}}" "$@"
    if [ -t 0 ] && [ -t 1 ]; then set -- -it "$@"; fi
    exec docker run --rm --pull=never --platform linux/amd64 --mount "type=bind,source={{justfile_directory()}},target=/workspace/impl_os" --workdir /workspace/impl_os "$@"

# [Host] Fresh Cargo build in the pristine official image, offline; no boot/tests.
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
