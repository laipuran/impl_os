# Make owns kernel artifacts; Just directly runs development commands.
set positional-arguments

contest_image := `sed -n 's/^ARG CONTEST_IMAGE=//p' .devcontainer/Dockerfile`
dev_image := "impl-os-dev:20260510"
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
    rustup toolchain install "{{toolchain}}" --no-self-update --profile minimal --component rustfmt,clippy,llvm-tools-preview --target riscv64gc-unknown-none-elf,loongarch64-unknown-none

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
    qemu-system-riscv64 --version
    qemu-system-loongarch64 --version
    gdb-multiarch --version

# [Container] Build both ELF files via Make; accepts MODE=debug, etc.
build *args:
    make all "$@"

# [Container] Build only kernel-rv; optional mode: release/debug.
build-rv mode="release":
    make kernel-rv MODE="$1"

# [Container] Build only kernel-la; optional mode: release/debug.
build-la mode="release":
    make kernel-la MODE="$1"

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

# [Container] Build release RISC-V and boot; args are native QEMU options.
run-rv *args: build-rv
    {{rv}} -kernel kernel-rv -m "{{memory}}" -smp "{{cpus}}" "$@"

# [Container] Build release LoongArch and boot; poweroff remains unsupported.
run-la *args: build-la
    {{la}} -kernel kernel-la -m "{{memory}}" -smp "{{cpus}}" "$@"

# [Container] Build a debug ELF and pause QEMU for GDB on :1234.
debug-rv *args: (build-rv "debug")
    {{rv}} -kernel kernel-rv -m "{{memory}}" -smp "{{cpus}}" -S -gdb tcp::1234 "$@"

# [Container] LoongArch equivalent of debug-rv.
debug-la *args: (build-la "debug")
    {{la}} -kernel kernel-la -m "{{memory}}" -smp "{{cpus}}" -S -gdb tcp::1234 "$@"

# [Host] Explicitly download the pinned official base image.
image-pull:
    docker pull --platform linux/amd64 "{{contest_image}}"

# [Host] Build the official base + just; do not replace compilers/QEMU/firmware.
image-build:
    docker build --platform linux/amd64 --tag "{{dev_image}}" --file .devcontainer/Dockerfile .

# [Host] Enter the development image or execute a command; never auto-pull.
container *command:
    #!/bin/sh
    set -eu
    if [ "$#" -eq 0 ]; then set -- bash; fi
    set -- --entrypoint /usr/bin/env "{{dev_image}}" "$@"
    if [ -t 0 ] && [ -t 1 ]; then set -- -it "$@"; fi
    exec docker run --rm --pull=never --platform linux/amd64 --mount "type=bind,source={{justfile_directory()}},target=/workspace/impl_os" --workdir /workspace/impl_os "$@"

# [Host] Fresh Cargo build in the pristine official image, offline; no boot/tests.
judge-build:
    docker run --rm --pull=never --platform linux/amd64 --network none --env RUSTUP_AUTO_INSTALL=0 --mount "type=bind,source={{justfile_directory()}},target=/workspace/impl_os" --workdir /workspace/impl_os --entrypoint make "{{contest_image}}" all TARGET_DIR=/tmp/impl-os-judge-target
