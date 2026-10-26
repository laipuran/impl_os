use std::{env, path::PathBuf};

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    assert!(
        matches!(arch.as_str(), "riscv64" | "loongarch64"),
        "kernel supports only riscv64 and loongarch64; use make all"
    );
    let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let script = dir.join("src/arch").join(arch).join("linker.ld");
    println!("cargo:rerun-if-changed={}", script.display());
    println!("cargo:rustc-link-arg=-T{}", script.display());
    println!("cargo:rustc-link-arg=--no-relax");
}
