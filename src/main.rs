//! BORUIX `threaddemo`: end-to-end multithread demo (threads.md T1-8 + T2-1 per-thread errno).
//!
//! T1-8 baseline: same-process thread_spawn derives two threads (thread-a/b) sharing the leader
//! ThreadGroup Arc addr space, each on its own mmap user stack, each thread_exit reaped by leader
//! thread_join; PASS requires correct shared counter + join codes.
//!
//! T2-1 addition: each thread installs its own libc Tcb (mmap, errno at offset 0) and sets
//! IA32_FS_BASE (kernel T2-0 saves/restores per thread) to it; libc errno writes/reads then hit
//! each thread own Tcb.errno. thread-a always sets ENOENT(2), thread-b EAGAIN(11); each re-reads
//! must stay its own value across yields; if FS base leaks across a switch (b reads a slot) FAIL.
//!
//! Thread entry ABI (T1-8 + T2-0 starter): kernel thread_spawn_with_starter(entry, stack, starter)
//! (0x35, a3=starter) puts starter in first-run rdi (= this thread Tcb addr). entry is a naked
//! shell keeping rdi then calls body(tcb). body never returns; thread_exit ends.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use libc::errno::{errno, set_errno};
use libsys::{mmap, thread_exit, thread_join, thread_spawn_with_starter, write, yield_now, STDOUT};

const THREAD_STACK_SIZE: u64 = 0x10000;
const TCB_SIZE: u64 = 0x1000;
const ENOENT: i32 = 2;
const EAGAIN: i32 = 11;
static SHARED_COUNTER: AtomicU64 = AtomicU64::new(0);
static ERRNO_FAILED: AtomicU8 = AtomicU8::new(0);
static mut PEER_B: u64 = 0;
static mut PEER_A: u64 = 0;

fn u64_to_dec(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    if v == 0 { buf[0] = b'0'; return &buf[..1]; }
    let mut i = buf.len();
    while v > 0 { i -= 1; buf[i] = b'0' + (v % 10) as u8; v /= 10; }
    &buf[i..]
}

fn td(s: &[u8]) {
    let _ = write(STDOUT, b"[threaddemo] ");
    let _ = write(STDOUT, s);
    let _ = write(STDOUT, b"\n");
}

/// thread-a body (T1-8 compute/print + T2-1 errno check). tcb = own Tcb addr (starter/rdi).
#[inline(never)]
fn thread_a_body(tcb: u64) -> ! {
    libc::thread::write_fs_base(tcb);
    for i in 0u32..5 {
        set_errno(ENOENT);
        for _ in 0..300_000u32 { let _x = (i as u64).wrapping_mul(6364136223846793005).wrapping_add(1); }
        let _ = yield_now();
        let own = errno();
        let peer = unsafe { *(PEER_B as *const i32) };
        if own != ENOENT || peer == ENOENT { ERRNO_FAILED.store(1, Ordering::SeqCst); }
        SHARED_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut b = [0u8; 20];
        let _ = write(STDOUT, b"[thread-a] i=");
        let _ = write(STDOUT, u64_to_dec(i as u64, &mut b));
        let _ = write(STDOUT, b" errno=");
        let _ = write(STDOUT, u64_to_dec(own as u64, &mut b));
        let _ = write(STDOUT, b"\n");
    }
    td(b"thread-a done, errno stayed 2, thread_exit(0)");
    thread_exit(0)
}

/// thread-b body (same, expect EAGAIN).
#[inline(never)]
fn thread_b_body(tcb: u64) -> ! {
    libc::thread::write_fs_base(tcb);
    for i in 0u32..5 {
        set_errno(EAGAIN);
        for _ in 0..250_000u32 { let _x = (i as u64).wrapping_mul(2862933555777941757).wrapping_add(3037000493); }
        let _ = yield_now();
        let own = errno();
        let peer = unsafe { *(PEER_A as *const i32) };
        if own != EAGAIN || peer == EAGAIN { ERRNO_FAILED.store(1, Ordering::SeqCst); }
        SHARED_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut b = [0u8; 20];
        let _ = write(STDOUT, b"[thread-b] i=");
        let _ = write(STDOUT, u64_to_dec(i as u64, &mut b));
        let _ = write(STDOUT, b" errno=");
        let _ = write(STDOUT, u64_to_dec(own as u64, &mut b));
        let _ = write(STDOUT, b"\n");
    }
    td(b"thread-b done, errno stayed 11, thread_exit(0)");
    thread_exit(0)
}

#[unsafe(naked)]
unsafe extern "C" fn thread_a_entry() {
    core::arch::naked_asm!("and rsp, -16", "call {b}", b = sym thread_a_body);
}
#[unsafe(naked)]
unsafe extern "C" fn thread_b_entry() {
    core::arch::naked_asm!("and rsp, -16", "call {b}", b = sym thread_b_body);
}

#[unsafe(no_mangle)]
pub extern "C" fn user_main(_argc: isize, _argv: *const *const u8) -> i32 {
    td(b"starting threaddemo (T1-8 two threads + T2-1 per-thread errno)...");
    let Ok(ra) = mmap(THREAD_STACK_SIZE) else { td(b"mmap stackA failed"); return 2; };
    let Ok(rb) = mmap(THREAD_STACK_SIZE) else { td(b"mmap stackB failed"); return 3; };
    let Ok(rc) = mmap(TCB_SIZE) else { td(b"mmap tcbA failed"); return 4; };
    let Ok(rd) = mmap(TCB_SIZE) else { td(b"mmap tcbB failed"); return 5; };
    let sa: u64 = ra + THREAD_STACK_SIZE;
    let sb: u64 = rb + THREAD_STACK_SIZE;
    let tcb_a: u64 = rc;
    let tcb_b: u64 = rd;
    unsafe { core::ptr::write_bytes(tcb_a as *mut u8, 0, 64); core::ptr::write_bytes(tcb_b as *mut u8, 0, 64); }
    unsafe { PEER_B = tcb_b; PEER_A = tcb_a; }
    let a_entry = thread_a_entry as unsafe extern "C" fn() as usize as u64;
    let b_entry = thread_b_entry as unsafe extern "C" fn() as usize as u64;
    let mut b = [0u8; 20];
    let _ = write(STDOUT, b"[threaddemo] tcb_a=0x");
    let _ = write(STDOUT, u64_to_dec(tcb_a, &mut b));
    let _ = write(STDOUT, b" tcb_b=0x");
    let _ = write(STDOUT, u64_to_dec(tcb_b, &mut b));
    let _ = write(STDOUT, b"\n");
    let tid_a = match thread_spawn_with_starter(a_entry, sa, tcb_a) { Ok(t) => t, Err(_) => { td(b"thread_spawn(a) failed"); return 6; } };
    let tid_b = match thread_spawn_with_starter(b_entry, sb, tcb_b) { Ok(t) => t, Err(_) => { td(b"thread_spawn(b) failed"); return 7; } };
    let _ = write(STDOUT, b"[threaddemo] spawned a tid=");
    let _ = write(STDOUT, u64_to_dec(tid_a, &mut b));
    let _ = write(STDOUT, b", b tid=");
    let _ = write(STDOUT, u64_to_dec(tid_b, &mut b));
    let _ = write(STDOUT, b"\n");
    let code_a = match join_thread(tid_a, 0) { Ok(c) => c, Err(_) => { td(b"thread_join(a) failed"); return 8; } };
    let code_b = match join_thread(tid_b, 0) { Ok(c) => c, Err(_) => { td(b"thread_join(b) failed"); return 9; } };
    let shared = SHARED_COUNTER.load(Ordering::SeqCst);
    let ebad = ERRNO_FAILED.load(Ordering::SeqCst);
    let _ = write(STDOUT, b"[threaddemo] join codes a=");
    let _ = write(STDOUT, u64_to_dec(code_a, &mut b));
    let _ = write(STDOUT, b" b=");
    let _ = write(STDOUT, u64_to_dec(code_b, &mut b));
    let _ = write(STDOUT, b" shared=");
    let _ = write(STDOUT, u64_to_dec(shared, &mut b));
    let _ = write(STDOUT, b" errno_failed=");
    let _ = write(STDOUT, u64_to_dec(ebad as u64, &mut b));
    let _ = write(STDOUT, b"\n");
    let pass: bool = shared == 10 && code_a == 0 && code_b == 0 && ebad == 0;
    if pass {
        td(b"threaddemo PASS (2 threads joined, shared addr-space, per-thread errno)");
        0
    } else {
        td(b"threaddemo FAIL (counter/join/errno mismatch)");
        10
    }
}

fn join_thread(tid: u64, attempt: u32) -> Result<u64, libsys::Error> {
    if attempt > 1_000_000 { return Err(libsys::Error::WouldBlock); }
    match thread_join(tid) {
        Ok(c) => Ok(c),
        Err(libsys::Error::WouldBlock) => { let _ = yield_now(); join_thread(tid, attempt + 1) }
        Err(e) => Err(e),
    }
}
