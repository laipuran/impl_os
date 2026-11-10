//! 构造小型 FDT 验证解析，不依赖宿主工具或实际物理地址。

mod builder;
mod invalid;
mod valid;

/// 为架构启动数据读取测试提供独立编码的有效 FDT，不调用待测解析器。
pub(crate) fn memory_blob() -> alloc::vec::Vec<u8> {
    builder::basic().finish()
}

/// 验证多 RAM 区间、保留区、initrd、错误描述和截断输入。
pub fn kernel_test() {
    valid::run();
    invalid::run();
}
