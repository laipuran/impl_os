//! Boot-time kernel tests, compiled only with the `ktest` feature.

/// Run every registered suite once after normal boot initialization.
///
/// Call on the boot CPU with interrupts disabled. Suites must restore temporary
/// mappings and machine state before returning; a failed assertion halts via
/// the kernel panic handler, so the final success marker is never printed.
pub fn run_all() {
    crate::arch::trap::smoke_test();
    crate::println!("{}: all kernel tests passed", crate::arch::NAME);
}
