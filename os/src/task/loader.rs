//! 在非活动地址空间中装载静态 ELF，并将初始上下文绑定到该地址空间。

use crate::{
    arch::trap::TrapContext,
    mm::{self, address_space::AddressSpace, frame::PAGE_SIZE, Access, UserMappingError},
};

use super::{elf, user_stack, ExitStatus, Task};

/// 静态用户程序装载失败。
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("ELF parse failed: {0}")]
    Elf(elf::Error),
    #[error("initial stack construction failed: {0}")]
    Stack(user_stack::Error),
    #[error("user mapping failed: {0}")]
    Memory(#[from] UserMappingError),
    #[error("unsupported ELF segment permissions")]
    UnsupportedPermissions,
}

/// 同一次装载产生的用户地址空间和初始任务。
pub(crate) struct LoadedProgram {
    space: AddressSpace<'static>,
    task: Task,
}

impl LoadedProgram {
    /// 装载静态 ELF，并构建初始用户栈。
    ///
    /// # Errors
    ///
    /// ELF、栈布局、映射范围或权限不受支持，或资源不足时返回错误。
    ///
    /// # Safety
    ///
    /// 当前须为关中断的启动主核，启动页框分配器已初始化且管理独占的真实 RAM。
    pub(crate) unsafe fn load(image: &[u8], argv: &[&[u8]], envp: &[&[u8]]) -> Result<Self, Error> {
        let elf = elf::parse(image).map_err(Error::Elf)?;
        let headers = elf.program_headers();
        let mut auxv = [
            user_stack::AuxEntry {
                key: user_stack::AT_ENTRY,
                value: elf.entry(),
            },
            user_stack::AuxEntry {
                key: user_stack::AT_PAGESZ,
                value: PAGE_SIZE,
            },
            user_stack::AuxEntry {
                key: user_stack::AT_PHENT,
                value: headers.entry_size,
            },
            user_stack::AuxEntry {
                key: user_stack::AT_PHNUM,
                value: headers.count,
            },
            user_stack::AuxEntry {
                key: user_stack::AT_PHDR,
                value: 0,
            },
        ];
        let count = if let Some(address) = headers.virtual_address {
            auxv[4].value = address;
            auxv.len()
        } else {
            auxv.len() - 1
        };
        // SAFETY: 调用方保证启动执行环境与全局分配器满足地址空间构建要求；栈构造回调只写页内缓冲区，不改变地址转换配置或开启并发。
        let (mut space, sp) = unsafe {
            AddressSpace::new_with_stack_in(mm::frames(), |stack| {
                user_stack::build(AddressSpace::STACK_TOP, stack, argv, envp, &auxv[..count])
                    .map_err(Error::Stack)
            })
        }?;
        for segment in elf.load_segments() {
            AddressSpace::check_range(segment.pages.start, segment.pages.end)
                .map_err(Error::Memory)?;
            let access = match (
                segment.permissions.read,
                segment.permissions.write,
                segment.permissions.execute,
            ) {
                (true, false, false) => Access::Read,
                (true, true, false) => Access::ReadWrite,
                (false, false, true) => Access::Execute,
                (true, false, true) => Access::ReadExecute,
                _ => return Err(Error::UnsupportedPermissions),
            };
            let mut data = segment.data;
            let mut offset = segment.data_offset;
            for address in segment.pages.step_by(PAGE_SIZE) {
                let (page, rest) = data.split_at(data.len().min(PAGE_SIZE - offset));
                space
                    .map_page(address, access, offset, page)
                    .map_err(Error::Memory)?;
                data = rest;
                offset = 0;
            }
            debug_assert!(data.is_empty());
        }
        let task = Task::new(TrapContext::user(elf.entry(), sp));
        Ok(Self { space, task })
    }

    /// 激活程序自己的地址空间并同步运行，返回时恢复启动页表。
    ///
    /// # Errors
    ///
    /// 地址空间未能激活时返回错误。
    ///
    /// # Safety
    ///
    /// 当前须为关中断的启动主核，Trap 已安装且没有嵌套用户态切换。
    /// 当前监管态栈（含保存在栈上的 Trap 现场）须位于内核镜像的启动栈，本对象须位于
    /// 切换前后同地址可访问的内核存储中。程序须退出或故障才会返回。
    pub(crate) unsafe fn run(&mut self) -> Result<ExitStatus, UserMappingError> {
        // SAFETY: 调用方保证启动栈、Trap 现场及任务对象持续可访问。任务循环
        // 不软件访问未映射表页；用户入口和用户栈均来自本地址空间持有的映射。
        unsafe { self.space.with_active(|| self.task.run()) }
    }
}

#[cfg(feature = "ktest")]
#[path = "../ktest/elf_execution/mod.rs"]
pub(crate) mod tests;
