//! BORUIX `threaddemo`：T1-8 端到端用户态多线程示例（threads.md T1-8 / ADR-035 阶段一收尾）。
//!
//! 目标：**真实用户态程序**在一个进程（组长 = 本 `user_main` 所在进程）内
//! `thread_spawn` 派生两个同组线程（thread-a / thread-b），两者共享组长线程组的
//! Arc 地址空间（同 cr3、同 /programs/threaddemo.elf 代码、同堆、同全局量），各自在
//! **独立 mmap 用户栈**上被内核调度到、独立运行、独立打印，最后各自 `thread_exit`
//! 由组长 `thread_join` 收尸，两 join 全成功后打印 PASS 并退出码 0。
//!
//! # 线程入口调用约定（本里程碑核心）
//!
//! 内核 `thread_spawn(entry, user_stack_top)`（SYS_TASK_THREAD_SPAWN/0x35）经
//! `initial_frame(entry, user_stack_top)`（kernel task/scheduler.rs）为该新调度单元装配
//! 首帧：首次被调度时以 `iretq` 直接进入 `entry`，`rsp = user_stack_top`，全部 GPR=0，
//! **栈上没有任何返回地址**、也无 `_start` 那样的 `and rsp,-16` 对齐。因此：
//!
//! 1. 线程入口是**无参裸函数**（不能依赖寄存器参数、不能依赖栈上返回地址）；
//! 2. 入口**必须永不正常返回**——Rust 侧以调用 `libsys::thread_exit(code)`
//!    （= `process::exit` 别名，SYS_TASK_EXIT）收尾；内核 terminate_locked 按
//!    调用方身份分流：组员调 = 仅该线程单体退出并留 zombie 供组长 join，不杀整组；
//! 3. **栈对齐由入口自己负责**：SysV 假定函数进入点（call 后）`rsp%16==8`，而 iretq
//!    进入时 `rsp=user_stack_top` 无对齐保证。故每个线程入口做成 `#[unsafe(naked)]`
//!    汇编薄壳：先 `and rsp,-16` 对齐，再 `call` 一个永不返回的普通 Rust 函数——
//!    该 call 压入返回地址使正文以标准 SysV 约定（rsp%16==8）进入，正文是普通可优化
//!    的 Rust 函数（含 libsys 写/算），无任何 prologue 对齐依赖。入口地址即传给
//!    内核的 naked 壳地址。
//!
//! # 栈来源（PRE-6 / ADR-035 D1/R5）
//!
//! 线程不新建地址空间、不 load ELF，只复用组长 Arc 地址空间；线程自己的用户栈由
//! 用户态用 `libsys::mmap(size)` 在组长地址空间内预留一段并按需分页区，栈顶 =
//! `mmap_start + size` 传给 `thread_spawn`（栈向下生长）。

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU64, Ordering};
use libsys::{mmap, thread_exit, thread_join, thread_spawn, write, yield_now, STDOUT};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 每线程独立用户栈区大小（字节）。用户态用 mmap 预留，栈顶 = 起点 + 此大小。
const THREAD_STACK_SIZE: u64 = 0x10000;

// ---------------------------------------------------------------------------
// 共享的 Arc 地址空间可见全局量（证明两线程共享组长地址空间）
// ---------------------------------------------------------------------------

/// 线程组内共享计数：thread-a / thread-b 各自原子累加，组长 join 后回读验证
/// "同一进程两个线程共享同一地址空间里的同一全局量"。
static SHARED_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// 打印辅助
// ---------------------------------------------------------------------------

/// 把 u64 写成十进制字节到 buf，返回有效切片（无前导零）。
fn u64_to_dec(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    if v == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut i = buf.len();
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    &buf[i..]
}

/// 输出 "[threaddemo] " 前缀行（组长/入口正文统一走这里，方便日志识别）。
fn td(s: &[u8]) {
    let _ = write(STDOUT, b"[threaddemo] ");
    let _ = write(STDOUT, s);
    let _ = write(STDOUT, b"\n");
}

// ---------------------------------------------------------------------------
// 线程正文（普通 Rust 函数，SysV 约定进入；永不返回，以 thread_exit 收尾）
// ---------------------------------------------------------------------------

/// thread-a 的正文：纯整数计算 + 数次打印 + 原子共享累加；结束 thread_exit(0)。
///
/// 以 `!` 返回型声明 → 编译器知道不返回，无需 epilogue，栈上无返回地址依赖。
#[inline(never)]
fn thread_a_body() -> ! {
    let mut acc: u64 = 0x1234_5678_9abc_def0;
    for i in 0u32..5 {
        // 一段有真实计算量的整数运算（使本线程在用户态确实"运行"而非瞬时完成）。
        for _ in 0..300_000u32 {
            acc = acc.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        SHARED_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut b = [0u8; 20];
        let s = u64_to_dec(i as u64, &mut b);
        let _ = write(STDOUT, b"[thread-a] i=");
        let _ = write(STDOUT, s);
        let _ = write(STDOUT, b" acc_lo=");
        let lo = u64_to_dec(acc & 0xffff, &mut b);
        let _ = write(STDOUT, lo);
        let _ = write(STDOUT, b"\n");
        // 主动让出 CPU，让调度器有机会切到 thread-b / 组长（真实并发调度证据）。
        let _ = yield_now();
    }
    td(b"thread-a done, thread_exit(0)");
    thread_exit(0)
}

/// thread-b 的正文：独立的一套整数计算 + 数次打印 + 原子共享累加；结束 thread_exit(0)。
#[inline(never)]
fn thread_b_body() -> ! {
    let mut acc: u64 = 0xfeed_beef_cafe_f00d;
    for i in 0u32..5 {
        for _ in 0..250_000u32 {
            acc = acc.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
        }
        SHARED_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut b = [0u8; 20];
        let s = u64_to_dec(i as u64, &mut b);
        let _ = write(STDOUT, b"[thread-b] i=");
        let _ = write(STDOUT, s);
        let _ = write(STDOUT, b" acc_lo=");
        let lo = u64_to_dec(acc & 0xffff, &mut b);
        let _ = write(STDOUT, lo);
        let _ = write(STDOUT, b"\n");
        let _ = yield_now();
    }
    td(b"thread-b done, thread_exit(0)");
    thread_exit(0)
}

// ---------------------------------------------------------------------------
// 线程入口（naked 薄壳：对齐栈 + call 永不返回的正文）
// ---------------------------------------------------------------------------

/// thread-a 线程入口：传给内核 thread_spawn 的地址即本符号地址。
///
/// iretq 直接进入本函数（rsp=user_stack_top、无返回地址）。先 `and rsp,-16`
/// 对齐栈，再 `call thread_a_body`（压返回地址 → 正文以标准 SysV rsp%16==8
/// 进入）。正文永不返回，故壳后无需 `ret`/处理。
#[unsafe(naked)]
unsafe extern "C" fn thread_a_entry() {
    core::arch::naked_asm!(
        "and rsp, -16",
        "call {body}",
        body = sym thread_a_body,
    );
}

/// thread-b 线程入口（同 thread-a）。
#[unsafe(naked)]
unsafe extern "C" fn thread_b_entry() {
    core::arch::naked_asm!(
        "and rsp, -16",
        "call {body}",
        body = sym thread_b_body,
    );
}

// ---------------------------------------------------------------------------
// 用户程序入口（组长 = 本进程）
// ---------------------------------------------------------------------------

/// 组长 user_main：
/// 1. 为 thread-a / thread-b 各自 mmap 一块独立用户栈（0x10000 字节），取栈顶；
/// 2. thread_spawn(a_entry, stackA_top) / thread_spawn(b_entry, stackB_top) 各得 tid；
/// 3. thread_join(tidA) / thread_join(tidB) 收退出码；
/// 4. 两 join 成功 → 回读共享计数 → 打印 PASS → 返回 0（进程退出码）。
#[unsafe(no_mangle)]
pub extern "C" fn user_main(_argc: isize, _argv: *const *const u8) -> i32 {
    td(b"starting T1-8 two-thread demo...");

    // 1) 每线程独立用户栈：mmap 0x10000 字节，栈顶 = 起点 + size（向下生长）。
    //    mmap 失败按硬错误处理——本 demo 是全栈验证，栈分配失败即如实失败。
    let stack_a_start = match mmap(THREAD_STACK_SIZE) {
        Ok(s) => s,
        Err(_) => {
            td(b"mmap(stack A) failed");
            return 2;
        }
    };
    let stack_b_start = match mmap(THREAD_STACK_SIZE) {
        Ok(s) => s,
        Err(_) => {
            td(b"mmap(stack B) failed");
            return 3;
        }
    };
    let stack_a_top = stack_a_start + THREAD_STACK_SIZE;
    let stack_b_top = stack_b_start + THREAD_STACK_SIZE;
    let mut b = [0u8; 20];
    td(b"stacks mapped:");
    let _ = write(STDOUT, b"[threaddemo]   stackA top=0x");
    let _ = write_hex(STDOUT, stack_a_top);
    let _ = write(STDOUT, b"\n");
    let _ = write(STDOUT, b"[threaddemo]   stackB top=0x");
    let _ = write_hex(STDOUT, stack_b_top);
    let _ = write(STDOUT, b"\n");

    // 2) 派生两个同组线程（共享本进程 Arc 地址空间，各自独立栈）。
    let a_entry = thread_a_entry as unsafe extern "C" fn() as usize as u64;
    let b_entry = thread_b_entry as unsafe extern "C" fn() as usize as u64;
    let tid_a = match thread_spawn(a_entry, stack_a_top) {
        Ok(t) => t,
        Err(_) => {
            td(b"thread_spawn(a) failed");
            return 4;
        }
    };
    let tid_b = match thread_spawn(b_entry, stack_b_top) {
        Ok(t) => t,
        Err(_) => {
            td(b"thread_spawn(b) failed");
            return 5;
        }
    };
    let _ = write(STDOUT, b"[threaddemo] spawned thread-a tid=");
    let _ = write(STDOUT, u64_to_dec(tid_a, &mut b));
    let _ = write(STDOUT, b", thread-b tid=");
    let _ = write(STDOUT, u64_to_dec(tid_b, &mut b));
    let _ = write(STDOUT, b"\n");

    // 3) join 收尸：组长阻塞等 thread-a / thread-b 各自退出并取退出码。
    //    若线程仍在运行 → 组长真阻塞（内核 waitpid 单目标交付）。两线程都与组长
    //    共享地址空间，故两 join 都在组长用户栈/地址空间内完成。
    let code_a = match thread_join(tid_a) {
        Ok(c) => c,
        Err(_) => {
            td(b"thread_join(a) failed");
            return 6;
        }
    };
    let code_b = match thread_join(tid_b) {
        Ok(c) => c,
        Err(_) => {
            td(b"thread_join(b) failed");
            return 7;
        }
    };
    let _ = write(STDOUT, b"[threaddemo] join(a) code=");
    let _ = write(STDOUT, u64_to_dec(code_a, &mut b));
    let _ = write(STDOUT, b", join(b) code=");
    let _ = write(STDOUT, u64_to_dec(code_b, &mut b));
    let _ = write(STDOUT, b"\n");

    // 4) 回读共享计数（两线程各累加 5 次 → 应为 10，证明共享地址空间 + 都被调度）。
    let shared = SHARED_COUNTER.load(Ordering::SeqCst);
    let _ = write(STDOUT, b"[threaddemo] shared counter = ");
    let _ = write(STDOUT, u64_to_dec(shared, &mut b));
    let _ = write(STDOUT, b" (expect 10)\n");

    if shared == 10 && code_a == 0 && code_b == 0 {
        td(b"threaddemo PASS (2 threads joined)");
        0
    } else {
        td(b"threaddemo FAIL (counter or join code mismatch)");
        8
    }
}

/// 写一段十六进制小写字符串到 fd（调试用，栈地址展示）。返回 write 结果。
fn write_hex(fd: u64, v: u64) -> Result<usize, libsys::Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut tmp = [0u8; 16];
    for i in 0..16 {
        tmp[15 - i] = HEX[((v >> (4 * i)) & 0xf) as usize];
    }
    // 去前导零
    let mut start = 0;
    while start < 15 && tmp[start] == b'0' {
        start += 1;
    }
    write(fd, &tmp[start..])
}
