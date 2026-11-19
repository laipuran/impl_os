//! 启动期 Linux 初始用户栈布局白盒自检。

use super::*;

const UNTOUCHED: u8 = 0xa5;

/// 检验用户栈字节布局、地址换算和报错时的缓冲区完整性。
pub(crate) fn kernel_test() {
    unaligned_stack_layout();
    exact_and_insufficient_capacity();
    empty_strings();
    rejected_inputs_leave_stack_untouched();
}

fn unaligned_stack_layout() {
    const BASE: usize = 0x1_0000_1ffb;
    let mut stack = [UNTOUCHED; 184];
    let auxv = [
        AuxEntry {
            key: AT_ENTRY,
            value: 0x401080,
        },
        AuxEntry {
            key: AT_PHNUM,
            value: 7,
        },
        AuxEntry {
            key: AT_PAGESZ,
            value: 4096,
        },
        AuxEntry {
            key: AT_PHDR,
            value: 0x400040,
        },
        AuxEntry {
            key: AT_PHENT,
            value: 56,
        },
    ];
    let sp = build(
        0x1_0000_20b3,
        &mut stack,
        &[b"prog", b"-q"],
        &[b"K=V", b"X=1"],
        &auxv,
    )
    .unwrap();

    assert_eq!(sp, 0x1_0000_2000);
    assert_eq!(stack[..5], [UNTOUCHED; 5]);
    let words = read_words::<19>(&stack[5..157]);
    assert_eq!(words[..4], [2, 0x1_0000_20a3, 0x1_0000_20a8, 0]); // argc, argv
    assert_eq!(words[4..7], [0x1_0000_20ab, 0x1_0000_20af, 0]); // envp
    assert_eq!(
        words[7..],
        [9, 0x401080, 5, 7, 6, 4096, 3, 0x400040, 4, 56, 0, 0], // auxv
    );
    assert_eq!(stack[157..168], [0; 11]);
    for (pointer, string) in [
        (words[1], &b"prog\0"[..]),
        (words[2], &b"-q\0"[..]),
        (words[4], &b"K=V\0"[..]),
        (words[5], &b"X=1\0"[..]),
    ] {
        assert_string_at(&stack, BASE, pointer, string);
    }
}

fn exact_and_insufficient_capacity() {
    let mut exact = [UNTOUCHED; 48];
    assert_eq!(build(0x1040, &mut exact, &[], &[], &[]), Ok(0x1010));
    assert_eq!(exact, [0; 48]);

    // 空栈的五个字占 40 字节，按虚拟地址对齐后需要 48 字节。
    let mut short = [UNTOUCHED; 47];
    for len in [39, 47] {
        assert_eq!(
            build(0x1040, &mut short[..len], &[], &[], &[]),
            Err(Error::InsufficientSpace)
        );
        assert_eq!(short, [UNTOUCHED; 47]);
    }
}

fn empty_strings() {
    let mut stack = [UNTOUCHED; 64];
    assert_eq!(build(0x1040, &mut stack, &[b""], &[b""], &[]), Ok(0x1000));
    let words = read_words::<7>(&stack[..56]);
    assert_eq!(words, [1, 0x103e, 0, 0x103f, 0, 0, 0]);
    assert_eq!(stack[56..62], [0; 6]);
    assert_string_at(&stack, 0x1000, words[1], b"\0");
    assert_string_at(&stack, 0x1000, words[3], b"\0");
}

fn rejected_inputs_leave_stack_untouched() {
    let assert_rejected = |top, argv: &[&[u8]], envp: &[&[u8]], auxv: &[AuxEntry], error| {
        let mut stack = [UNTOUCHED; 128];
        assert_eq!(build(top, &mut stack, argv, envp, auxv), Err(error));
        assert_eq!(stack, [UNTOUCHED; 128]);
    };

    assert_rejected(127, &[b"prog"], &[], &[], Error::AddressUnderflow);
    assert_rejected(0x1040, &[b"prog", b"a\0b"], &[], &[], Error::InteriorNul);
    assert_rejected(0x1040, &[b"prog"], &[b"K=\0"], &[], Error::InteriorNul);
    assert_rejected(
        0x1040,
        &[b"prog"],
        &[b"K=V"],
        &[
            AuxEntry {
                key: AT_PAGESZ,
                value: 4096,
            },
            AuxEntry {
                key: AT_NULL,
                value: 0,
            },
        ],
        Error::ReservedAuxKey,
    );
}

fn read_words<const N: usize>(bytes: &[u8]) -> [usize; N] {
    core::array::from_fn(|index| {
        let offset = index * 8;
        usize::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    })
}

fn assert_string_at(stack: &[u8], base: usize, pointer: usize, expected: &[u8]) {
    let offset = pointer - base;
    assert_eq!(&stack[offset..offset + expected.len()], expected);
}
