# threaddemo — BORUIX T1-8 端到端用户态多线程示例

独立用户程序 repo（一独立 repo 一程序），实现 threads.md **T1-8** / ADR-035 阶段一收尾：
**真实用户态程序**在同一进程（组长 = 本 `user_main` 所在进程）内派生两个同组线程
（thread-a / thread-b），二者共享组长线程组的 **Arc 地址空间**，各自在**独立 mmap 用户栈**
上被内核调度运行、独立打印，随后各自 `thread_exit`，组长 `thread_join` 收尸，QEMU 实跑 0 panic。

## 结构

- `src/main.rs`：程序主体（入口调用约定见下）。
- `Cargo.toml` / `build.rs` / `linker.ld`：同 synce2e 制（no_std/no_main、
  `panic=abort`、`-no-pie` ET_EXEC、链接脚本定位 0x400000）。
- 依赖 `libsys`（`path="../libsys"`）当前 thread 原语：`thread_spawn(entry,user_stack_top)`、
  `thread_join(tid)`、`thread_exit(code)`；`mem.rs::mmap(size)`。

## 逻辑（user_main，组长进程）

1. 为每个线程用 `libsys::mmap(0x10000)` 分配**独立用户栈区**，栈顶 = `mmap_start + size`
   （PRE-6：用户态自备栈、传栈顶）。
2. `thread_spawn(thread_a_entry, stackA_top)`、`thread_spawn(thread_b_entry, stackB_top)`
   各得一个 tid；两线程与组长同组、共享 Arc 地址空间（同 cr3 / 同代码 / 同全局量
   `SHARED_COUNTER`），各自独立栈上跑。
3. thread-a / thread-b 各做一段**纯整数计算**并打印 `[thread-a] i=...` / `[thread-b] i=...`
   数次，累加共享 `SHARED_COUNTER`（原子，证明共享地址空间），最后 `thread_exit(0)` 退出。
4. 组长 `thread_join(tid_a)`、`thread_join(tid_b)` 各收退出码；全成功后打印
   `threaddemo PASS (2 threads joined)` 并返回 0（进程退出码）。

## 线程入口调用约定（本里程碑核心验证）

内核 `thread_spawn`（SYS_TASK_THREAD_SPAWN/0x35）为该新调度单元装配 `initial_frame(entry,
user_stack_top)`：首次调度 **iretq 直接进 entry**、`rsp=user_stack_top`、全 GPR=0、**栈上无返回地址**
（无 `_start`）。因此入口是无参裸函数且**必须永不正常返回**；本示例的解：

- 每个线程入口是 `#[unsafe(naked)] unsafe extern "C" fn()` 汇编薄壳，先 `and rsp,-16`
  （自己负责栈对齐），再 `call` 一个永不返回的普通 Rust 正文函数（`-> !`，以
  `libsys::thread_exit(code)` 收尾）。call 压入返回地址 → 正文以标准 SysV
  `rsp%16==8` 约定进入，正文可正常用 libsys 写/算，无任何 prologue 对齐依赖。
- 传内核的 `entry` = naked 壳的函数地址（objdump 证实壳 = `and rsp,-16; call body`）。
- 组长/组员身份由内核 terminate_locked 按 tgid≠pid 分流：组员 `thread_exit` 只单体退、
  留 zombie 供组长 `thread_join` 收尸，不杀整组。

## 启动路径

由 PID 1（init）在 supervisor 循环前的启动序列里 `exec_path("/programs/threaddemo.elf")`
派生一次，并 `waitpid_any` 专候收尸（放跨核风暴之前，避免风暴的 waitpid_any 误收其僵尸）。

## 构建 / 运行

- SDK 构建（build.py 已登记 threaddemo）：
  `python sdk/main.py build`（生成 liveCD ISO，内嵌 threaddemo.elf）
- 实跑（QEMU，单核或 SMP4）：`python sdk/main.py run --serial --no-smp` /
  `--smp 4`；日志中可见两线程各自输出、join、`threaddemo PASS`，且 0 panic。
