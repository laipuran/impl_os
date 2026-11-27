//! 设备树内存与保留区解析自检（基于内存中合成的测试 DTB，不依赖外部工具或真实物理地址）。

// 本文件由 `os/src/mm/fdt.rs` 通过 `#[path = "../ktest/mm/fdt.rs"] pub mod tests;` 挂载，
// `rustc` 会以 `os/src/ktest/mm/` 为基目录查找子模块，因此须显式指定 `fdt/` 子路径。
#[path = "fdt/invalid.rs"]
mod invalid;
#[path = "fdt/valid.rs"]
mod valid;

/// 运行合法设备树与非法/损坏设备树的全部解析测试。
pub fn kernel_test() {
    valid::run();
    invalid::run();
}
