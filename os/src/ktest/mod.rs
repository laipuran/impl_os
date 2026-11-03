//! Boot-time kernel tests, compiled only with the `ktest` feature.

struct Suite {
    name: &'static str,
    run: fn(),
}

// Keep suite selection separate from the runner and the normal boot path.
const SUITES: &[Suite] = &[Suite {
    name: "trap",
    run: crate::arch::trap::tests::smoke_test,
}];

/// Run the configured suites in declaration order after normal initialization.
///
/// Call on the boot CPU with interrupts disabled. Suites must restore temporary
/// mappings and machine state before returning; a failed assertion halts via
/// the kernel panic handler, so the final success marker is never printed.
pub fn run() {
    for suite in SUITES {
        crate::println!("{}: running {} tests", crate::arch::NAME, suite.name);
        (suite.run)();
    }
    crate::println!("{}: all kernel tests passed", crate::arch::NAME);
}
