//! Register-preserving detour entry stubs.
//!
//! The game is built with whole-program optimisation, so for *direct* calls to
//! small functions the compiler knows exactly which registers the callee
//! touches and keeps live values in the others, including registers the Win64
//! ABI calls volatile. Example: the request retry loop in `FUN_140072b90`
//! keeps the network manager in `r11` and a counter in `r10` across
//! `call HttpRequest::Submit`, because `Submit` never touches them. A Rust
//! detour does, which crashed the game.
//!
//! [`preserving_detour!`] generates a naked entry that saves every volatile
//! register (`rcx rdx r8-r11`, `xmm0-xmm5`), calls a Rust handler with the
//! first argument, lets the handler replace `rcx`, restores everything and
//! tail-jumps to the original, so the original runs with the caller's exact
//! register state. Use it for any hook on a function the game calls directly.
//! Functions only reached through vtables or delegates are called with the
//! standard ABI and can use plain Rust detours.

/// `preserving_detour!(entry_name, handler, ORIGINAL)`:
/// * `handler: unsafe extern "C" fn(rcx: usize) -> usize` returns the value
///   to pass on in `rcx` (return the argument to leave it unchanged).
/// * `ORIGINAL: AtomicUsize` holds the MinHook trampoline.
macro_rules! preserving_detour {
    ($entry:ident, $handler:path, $original:path) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $entry() {
            core::arch::naked_asm!(
                // Entry: rsp = 8 mod 16. Six pushes keep it at 8 mod 16, so
                // 0x88 (8 mod 16) realigns it for the call.
                "push rcx",
                "push rdx",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "sub rsp, 0x88",
                // [rsp+0x00..0x20) is the callee's shadow space.
                "movdqu [rsp + 0x20], xmm0",
                "movdqu [rsp + 0x30], xmm1",
                "movdqu [rsp + 0x40], xmm2",
                "movdqu [rsp + 0x50], xmm3",
                "movdqu [rsp + 0x60], xmm4",
                "movdqu [rsp + 0x70], xmm5",
                "call {handler}",
                // Replace the saved rcx (the first push) with the handler's result.
                "mov [rsp + 0xb0], rax",
                "movdqu xmm0, [rsp + 0x20]",
                "movdqu xmm1, [rsp + 0x30]",
                "movdqu xmm2, [rsp + 0x40]",
                "movdqu xmm3, [rsp + 0x50]",
                "movdqu xmm4, [rsp + 0x60]",
                "movdqu xmm5, [rsp + 0x70]",
                "add rsp, 0x88",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rdx",
                "pop rcx",
                "jmp qword ptr [rip + {original}]",
                handler = sym $handler,
                original = sym $original,
            )
        }
    };
}

pub(crate) use preserving_detour;

/// `around_detour!(entry_name, pre, post, ORIGINAL)`: runs `pre()` before and
/// `post()` after the original, invisibly to a caller that relies on the
/// original's exact register use.
///
/// The original is called with the caller's exact volatile registers. After
/// it returns, every volatile register except `rax`/`xmm0` (the results) is
/// put back to its value from before the call: registers the original did
/// not touch end up unchanged, which is all an LTCG caller can rely on.
/// Only for functions taking at most four arguments (stack arguments would
/// be at the wrong offset for the inner call).
macro_rules! around_detour {
    ($entry:ident, $pre:path, $post:path, $original:path) => {
        #[unsafe(naked)]
        pub unsafe extern "C" fn $entry() {
            core::arch::naked_asm!(
                // Entry: rsp = 8 mod 16. Six pushes keep it at 8 mod 16;
                // 0xa8 (8 mod 16) realigns it.
                "push rcx",
                "push rdx",
                "push r8",
                "push r9",
                "push r10",
                "push r11",
                "sub rsp, 0xa8",
                // [rsp+0x00..0x20) shadow space, [0x20..0x80) xmm0-5,
                // [0x80..0x90) result xmm0, [0x90] result rax.
                // Saved GPRs: r11 0xa8, r10 0xb0, r9 0xb8, r8 0xc0, rdx 0xc8, rcx 0xd0.
                "movdqu [rsp + 0x20], xmm0",
                "movdqu [rsp + 0x30], xmm1",
                "movdqu [rsp + 0x40], xmm2",
                "movdqu [rsp + 0x50], xmm3",
                "movdqu [rsp + 0x60], xmm4",
                "movdqu [rsp + 0x70], xmm5",
                "call {pre}",
                "movdqu xmm0, [rsp + 0x20]",
                "movdqu xmm1, [rsp + 0x30]",
                "movdqu xmm2, [rsp + 0x40]",
                "movdqu xmm3, [rsp + 0x50]",
                "movdqu xmm4, [rsp + 0x60]",
                "movdqu xmm5, [rsp + 0x70]",
                "mov r11, [rsp + 0xa8]",
                "mov r10, [rsp + 0xb0]",
                "mov r9, [rsp + 0xb8]",
                "mov r8, [rsp + 0xc0]",
                "mov rdx, [rsp + 0xc8]",
                "mov rcx, [rsp + 0xd0]",
                "call qword ptr [rip + {original}]",
                "mov [rsp + 0x90], rax",
                "movdqu [rsp + 0x80], xmm0",
                "call {post}",
                "mov rax, [rsp + 0x90]",
                "movdqu xmm0, [rsp + 0x80]",
                "movdqu xmm1, [rsp + 0x30]",
                "movdqu xmm2, [rsp + 0x40]",
                "movdqu xmm3, [rsp + 0x50]",
                "movdqu xmm4, [rsp + 0x60]",
                "movdqu xmm5, [rsp + 0x70]",
                "add rsp, 0xa8",
                "pop r11",
                "pop r10",
                "pop r9",
                "pop r8",
                "pop rdx",
                "pop rcx",
                "ret",
                pre = sym $pre,
                post = sym $post,
                original = sym $original,
            )
        }
    };
}

pub(crate) use around_detour;

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    static ORIGINAL: AtomicUsize = AtomicUsize::new(0);

    /// Stands in for the hooked function: returns its rcx.
    #[unsafe(naked)]
    unsafe extern "C" fn fake_original() {
        core::arch::naked_asm!("mov rax, rcx", "ret")
    }

    /// Clobbers everything a Rust detour might, and replaces rcx.
    unsafe extern "C" fn clobbering_handler(rcx: usize) -> usize {
        core::arch::asm!(
            "mov r10, 0xdead", "mov r11, 0xbeef", "pxor xmm5, xmm5",
            out("r10") _, out("r11") _, out("xmm5") _,
        );
        rcx + 1
    }

    preserving_detour!(entry, clobbering_handler, ORIGINAL);

    /// Standard-ABI harness: loads known values into r10/r11/xmm5, calls the
    /// stub with rcx = 41, and writes rax/r10/r11/xmm5 to `out[0..4]`.
    #[unsafe(naked)]
    unsafe extern "C" fn harness(out: *mut [u64; 4]) {
        core::arch::naked_asm!(
            "push rbx",          // rsp now 16-aligned
            "mov rbx, rcx",
            "mov r10, 0x1111",
            "mov r11, 0x2222",
            "mov rax, 0x3333",
            "movq xmm5, rax",
            "mov rcx, 41",
            "call {entry}",
            "mov [rbx], rax",
            "mov [rbx + 8], r10",
            "mov [rbx + 16], r11",
            "movq [rbx + 24], xmm5",
            "pop rbx",
            "ret",
            entry = sym entry,
        )
    }

    static AROUND_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
    static CALLS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

    /// Stands in for an LTCG callee: returns rcx + rdx in rax and xmm0,
    /// clobbers r9 (so the caller cannot rely on it), leaves r10/r11 alone.
    #[unsafe(naked)]
    unsafe extern "C" fn around_original() {
        core::arch::naked_asm!("lea rax, [rcx + rdx]", "movq xmm0, rax", "mov r9, 0x9999", "ret")
    }

    unsafe extern "C" fn pre() {
        core::arch::asm!("mov r10, 0xdead", "mov r11, 0xbeef", "pxor xmm1, xmm1", out("r10") _, out("r11") _, out("xmm1") _);
        CALLS.lock().unwrap().push("pre");
    }

    unsafe extern "C" fn post() {
        core::arch::asm!("mov r10, 0xdead", "pxor xmm0, xmm0", out("r10") _, out("xmm0") _);
        CALLS.lock().unwrap().push("post");
    }

    around_detour!(around_entry, pre, post, AROUND_ORIGINAL);

    /// rcx = 40, rdx = 2, r10/r11/xmm1 = known; writes rax, r10, r11, xmm0,
    /// xmm1 to `out[0..5]`.
    #[unsafe(naked)]
    unsafe extern "C" fn around_harness(out: *mut [u64; 5]) {
        core::arch::naked_asm!(
            "push rbx",
            "mov rbx, rcx",
            "mov r10, 0x1111",
            "mov r11, 0x2222",
            "mov rax, 0x3333",
            "movq xmm1, rax",
            "mov rcx, 40",
            "mov rdx, 2",
            "call {entry}",
            "mov [rbx], rax",
            "mov [rbx + 8], r10",
            "mov [rbx + 16], r11",
            "movq [rbx + 24], xmm0",
            "movq [rbx + 32], xmm1",
            "pop rbx",
            "ret",
            entry = sym around_entry,
        )
    }

    #[test]
    fn around_stub_is_transparent() {
        AROUND_ORIGINAL.store(around_original as *const () as usize, Ordering::SeqCst);
        let mut out = [0u64; 5];
        unsafe { around_harness(&mut out) };
        assert_eq!(*CALLS.lock().unwrap(), ["pre", "post"]);
        assert_eq!(out[0], 42, "original sees the caller's arguments; rax survives post");
        assert_eq!(out[3], 42, "xmm0 result survives post");
        assert_eq!([out[1], out[2], out[4]], [0x1111, 0x2222, 0x3333], "untouched volatiles come back");
    }

    #[test]
    fn stub_preserves_volatile_registers() {
        ORIGINAL.store(fake_original as *const () as usize, Ordering::SeqCst);
        let mut out = [0u64; 4];
        unsafe { harness(&mut out) };
        assert_eq!(out[0], 42, "original must see the handler's rcx");
        assert_eq!(out[1..], [0x1111, 0x2222, 0x3333], "r10, r11, xmm5 must survive");
    }
}
