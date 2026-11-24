//! 在非活动地址空间中装载静态 ELF，并将初始上下文绑定到该地址空间。

use crate::{
    arch::trap::TrapContext,
    mm::{
        self,
        address_space::{AddressSpace, BuildError},
        frame::{FrameAllocator, PAGE_SIZE},
        Access, UserMappingError,
    },
};

use super::{elf, user_stack, ExitStatus, Task};

/// 静态用户程序装载失败。
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Error {
    /// 镜像未通过 ELF 校验。
    #[error("invalid static ELF image: {0}")]
    InvalidElf(#[source] elf::Error),
    /// 参数、环境变量及辅助向量无法放入启动栈。
    #[error("initial stack is too large")]
    StackTooLarge,
    /// 参数或环境变量含内嵌空字节。
    #[error("initial stack arguments or environment contain a NUL byte")]
    InvalidStackArguments,
    /// 装载器提供的栈地址或辅助向量不满足栈布局约束。
    #[error("invalid initial stack setup")]
    StackSetupFailed,
    /// 加载段的映射页区间超出普通用户映射范围。
    #[error("invalid ELF segment address")]
    InvalidSegmentAddress,
    /// 当前寻址模式不支持构建用户地址空间。
    #[error("user paging is unavailable in the current mode")]
    PagingUnavailable,
    /// 页框或映射元数据不足。
    #[error("out of memory for the loaded program")]
    OutOfMemory,
    /// 地址、内容或权限不满足页映射约束。
    #[error("failed to map a user page: {0}")]
    MappingFailed(#[source] UserMappingError),
    /// 加载段违反 W^X、只允许写入或不可访问。
    #[error("unsupported ELF segment permissions")]
    UnsupportedPermissions,
}

/// 同一次装载产生的用户地址空间和初始任务。
pub(crate) struct LoadedProgram<'a> {
    space: AddressSpace<'a>,
    task: Task,
}

impl LoadedProgram<'static> {
    /// 使用全局页框分配器装载静态 ELF，并在物理栈页上原位构建初始用户栈。
    ///
    /// 当前仅承诺最小启动 ABI：提供 `AT_ENTRY`、`AT_PAGESZ`，并在整张程序头表完整落在可读（`PF_R`）加载段内时成组提供 `AT_PHDR`、`AT_PHENT`、`AT_PHNUM`；在接入可信熵源前不提供 `AT_RANDOM`。
    ///
    /// # Errors
    ///
    /// - ELF 校验失败时返回 [`Error::InvalidElf`]；加载段超出普通用户映射范围时返回 [`Error::InvalidSegmentAddress`]。
    /// - 段权限违反 W^X、只允许写入或不可访问时返回 [`Error::UnsupportedPermissions`]。
    /// - 启动栈放不下时返回 [`Error::StackTooLarge`]；参数或环境变量含内嵌空字节时返回 [`Error::InvalidStackArguments`]；栈布局前提不满足时返回 [`Error::StackSetupFailed`]。
    /// - 当前寻址模式不支持构建时返回 [`Error::PagingUnavailable`]；资源不足时返回 [`Error::OutOfMemory`]；其余映射失败时返回 [`Error::MappingFailed`]。
    ///
    /// 失败时销毁未完成的地址空间并回收本次已分配的全部页框。
    ///
    /// # Panics
    ///
    /// 内存管理尚未初始化时 panic（见 [`crate::mm::frames`]）。
    ///
    /// # Safety
    ///
    /// - 仅在关中断的启动主核特权态调用和使用，并满足 [`AddressSpace::with_stack_in`] 的当前架构前提。
    /// - 全局页框分配器必须已初始化，并提供可由当前架构内核指针独占读写的真实 RAM 页框。
    #[cfg_attr(
        not(feature = "ktest"),
        expect(dead_code, reason = "正常启动尚不装载用户程序")
    )]
    pub(crate) unsafe fn load(image: &[u8], argv: &[&[u8]], envp: &[&[u8]]) -> Result<Self, Error> {
        // SAFETY: 调用方保证启动执行环境及全局分配器满足装载要求。
        unsafe { Self::load_in(mm::frames(), image, argv, envp) }
    }
}

impl<'a> LoadedProgram<'a> {
    /// 使用指定页框分配器装载静态 ELF，并在物理栈页上原位构建初始用户栈。
    ///
    /// # Errors
    ///
    /// 返回与 [`LoadedProgram::load`] 相同的装载错误，失败时回收本次已分配的全部页框。
    ///
    /// # Safety
    ///
    /// 调用方须满足 [`AddressSpace::with_stack_in`] 对执行环境和分配器的全部安全前提。
    unsafe fn load_in(
        frames: &'a FrameAllocator,
        image: &[u8],
        argv: &[&[u8]],
        envp: &[&[u8]],
    ) -> Result<Self, Error> {
        let elf = elf::parse(image).map_err(Error::InvalidElf)?;
        // SAFETY: 调用方保证启动执行环境和分配器满足地址空间构建要求；栈构造回调只处理数据，不改变地址转换配置，也不启用中断或并发。
        let (mut space, sp) = unsafe {
            AddressSpace::with_stack_in(frames, |buf| build_initial_stack(&elf, buf, argv, envp))
        }
        .map_err(|error| match error {
            BuildError::Mapping(error) => mapping_error(error),
            BuildError::Init(error) => match error {
                user_stack::Error::SizeOverflow | user_stack::Error::InsufficientSpace => {
                    Error::StackTooLarge
                }
                user_stack::Error::InteriorNul => Error::InvalidStackArguments,
                user_stack::Error::AddressUnderflow | user_stack::Error::ReservedAuxKey => {
                    Error::StackSetupFailed
                }
            },
        })?;
        for segment in elf.load_segments() {
            map_segment(&mut space, segment)?;
        }
        let task = Task::new(TrapContext::user(elf.entry(), sp));
        Ok(Self { space, task })
    }

    /// 激活程序自己的地址空间并同步运行，返回时恢复原地址空间。
    ///
    /// 不抢占用户程序，直至其主动退出或触发未处理故障才返回。
    ///
    /// # Errors
    ///
    /// 当前寻址模式不支持激活或硬件未接受目标分页配置时返回 [`UserMappingError::UnavailableInCurrentMode`]，并恢复原状态且不执行任务。
    ///
    /// # Safety
    ///
    /// - 当前须为关中断的启动主核特权态，满足 [`AddressSpace::with_active`] 与 [`Task::run`] 的全部硬件、Trap 初始化及无嵌套切换前提。
    /// - 内核栈（含 Trap 保存现场）须位于内核镜像的启动栈，本对象须在切换前后保持持续可访问。
    /// - 执行期间不得在其他路径改写地址转换配置、开启中断或并发。任务闭包只返回退出状态，不返回依赖临时活动映射的引用。
    #[cfg_attr(
        not(feature = "ktest"),
        expect(dead_code, reason = "正常启动尚不运行用户程序")
    )]
    pub(crate) unsafe fn run(&mut self) -> Result<ExitStatus, UserMappingError> {
        // SAFETY: 调用方保证当前执行环境、启动栈、Trap 现场和任务对象符合要求。任务循环不访问未映射的表页；入口和栈均由本地址空间映射。
        unsafe { self.space.with_active(|| self.task.run()) }
    }
}

fn build_initial_stack(
    elf: &elf::Elf<'_>,
    buf: &mut [u8; PAGE_SIZE],
    argv: &[&[u8]],
    envp: &[&[u8]],
) -> Result<usize, user_stack::Error> {
    use user_stack::{AuxEntry, AT_ENTRY, AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM};

    let entry = AuxEntry {
        key: AT_ENTRY,
        value: elf.entry(),
    };
    let page_size = AuxEntry {
        key: AT_PAGESZ,
        value: PAGE_SIZE,
    };
    let headers = elf.program_headers();
    let auxv: &[AuxEntry] = if let Some(address) = headers.virtual_address {
        &[
            entry,
            page_size,
            AuxEntry {
                key: AT_PHDR,
                value: address,
            },
            AuxEntry {
                key: AT_PHENT,
                value: headers.entry_size,
            },
            AuxEntry {
                key: AT_PHNUM,
                value: headers.count,
            },
        ]
    } else {
        &[entry, page_size]
    };
    // 没有可信熵源，不提供 AT_RANDOM。
    user_stack::build(AddressSpace::STACK_TOP, buf, argv, envp, auxv)
}

fn segment_access(permissions: elf::Permissions) -> Result<Access, Error> {
    match (permissions.read, permissions.write, permissions.execute) {
        (true, false, false) => Ok(Access::Read),
        (true, true, false) => Ok(Access::ReadWrite),
        (false, false, true) => Ok(Access::Execute),
        (true, false, true) => Ok(Access::ReadExecute),
        _ => Err(Error::UnsupportedPermissions),
    }
}

fn map_segment(space: &mut AddressSpace<'_>, segment: elf::LoadSegment<'_>) -> Result<(), Error> {
    AddressSpace::check_range(segment.pages.start, segment.pages.end)
        .map_err(|_| Error::InvalidSegmentAddress)?;
    let access = segment_access(segment.permissions)?;
    let mut data = segment.data;
    let mut offset = segment.data_offset;
    for address in segment.pages.step_by(PAGE_SIZE) {
        let (page, rest) = data.split_at(data.len().min(PAGE_SIZE - offset));
        space
            .map_page(address, access, offset, page)
            .map_err(mapping_error)?;
        data = rest;
        offset = 0;
    }
    debug_assert!(data.is_empty());
    Ok(())
}

fn mapping_error(error: UserMappingError) -> Error {
    match error {
        UserMappingError::InvalidUserAddress
        | UserMappingError::InitialDataTooLarge
        | UserMappingError::AlreadyMapped
        | UserMappingError::InvalidMapping => Error::MappingFailed(error),
        UserMappingError::UnavailableInCurrentMode => Error::PagingUnavailable,
        UserMappingError::OutOfMemory => Error::OutOfMemory,
    }
}

#[cfg(feature = "ktest")]
#[path = "../ktest/task/loader.rs"]
pub(crate) mod tests;
