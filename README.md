# threaddemo

**简体中文** | [English](#english)

BORUIX 的**用户态多线程示例**——演示并验证一个进程内多个线程的真实行为。

它在一个进程里派生两个线程，让它们各自在**独立的栈**上运行、共享同一份地址空间、拥有**各自
私有的线程局部数据**，最后汇合收尾。

```
[threaddemo] thread-a tid=... running
[threaddemo] thread-b tid=... running
[threaddemo] PASS (2 threads joined)
```

---

## 它验证什么

线程不是一个孤立的功能，它是四件事同时成立：

| 验证项 | 含义 |
| --- | --- |
| **共享地址空间** | 同一进程的线程看到**同一份**全局变量与代码 |
| **独立栈** | 每个线程有自己的栈，互不覆盖 |
| **独立线程局部存储** | 每线程的局部数据**互不干扰**，切换后仍是自己的 |
| **线程身份** | 每个线程有自己可查询的标识，且能查到所属进程 |

前两项直觉上容易理解，后两项才是容易出错的地方，下面分开说。

## 共享与独立

线程与进程的根本区别在于**什么被共享、什么不共享**。这个示例把两者同时演示出来：

- 两个线程累加**同一个**全局计数器，结束后总数正确 —— 证明地址空间是共享的
- 两个线程各自在**独立的栈**上运行 —— 证明栈是不共享的

如果栈被错误地共享，两个线程会互相破坏对方的调用帧，程序在运行中就会崩溃或产生乱序输出。
所以"每个线程一个栈"必须先分配好，再把栈顶传给派生接口。

## 线程局部数据：容易出错的地方

每个线程需要**自己的**一份数据——典型例子是错误码。如果两个线程共用一个错误码变量，那么
线程 A 设置的值会被线程 B 覆盖，A 读到的是别人的错误。

这个示例专门验证这一点，而且验证方式是**对抗性的**：

1. 两个线程设置**不同**的错误码
2. 各自主动让出 CPU（制造调度切换）
3. 醒来后各自读回

**同时还要读一次对方的**：如果自己读到的值是对的、但对方的值和自己一样，说明两者实际上在用
同一份存储——只是恰好值相同而已。两个方向都检查，才能排除"看起来对"的假象。

线程局部存储的验证用同样的思路：每个线程从**自己的**存储区分配槽位、写入独有标记，跨越调度
切换后标记必须还是自己的。此外还检查两个线程的存储区**互不重叠**——如果分配器错误地把同一
块区域给了两个线程，标记就会互相覆盖。

## 线程入口的调用约定

这是这个示例最关键的技术点，也是最容易踩坑的地方。

新线程启动时与"函数被调用"**不是一回事**。函数调用会在栈上压入返回地址，函数返回时跳回去；
而新线程的栈是**全新的**，上面**没有返回地址**——没有东西可以返回。

因此线程入口必须满足两条：

| 要求 | 原因 |
| --- | --- |
| **永不正常返回** | 栈上没有返回地址，返回即跳到未知位置 |
| **自行处理栈对齐** | 没有调用者替你准备栈帧 |

示例里的做法是：入口是一个极薄的汇编壳，先把栈对齐，再调用一个永不返回的正文函数（它以
"结束线程"收尾）。这样正文函数可以像普通代码一样书写，不必关心自己是被特殊方式启动的。

**给写同类程序的人的提示**：如果你写的线程入口试图正常 `return`，程序会跳到无效地址。这是
这套接口最容易出问题的地方。

## 线程身份

每个线程都能查询自己的标识，并且能查到它**属于哪个进程**。

这两个值必须**不同**：标识在线程之间唯一，而所属进程在所有线程里**相同**。如果实现错误地把
两者当成一回事，那么用"属于哪个进程"来做的判断（比如信号投递、资源归属）就会出错。

示例检查了三件事：主线程的标识与所属进程一致（它是唯一的线程）、两个子线程各自的标识互不相同
且与主线程不同、以及两个子线程查询到的所属进程**完全相同**。

## 运行方式

由系统初始化进程在启动序列中拉起一次，并等待它结束、收取退出码。

**它是一次性的验证负载**：跑完即退出，不常驻。

## 输出与结论

每个线程打印自己的运行过程，最后打印汇合结果。

成功的标志是打印 `PASS (2 threads joined)` 并以 0 退出。任何一项验证失败都会打印具体的失败项
并返回非零退出码。

## 构建

```bash
cargo build --release
```

编译产物部署为 BORUIX 系统中的用户态程序。

## 文件结构

```
threaddemo/
├── Cargo.toml    # 包定义
├── build.rs      # 注入链接脚本
├── linker.ld     # 用户态段布局
└── src/
    └── main.rs   # 两个线程的派生、验证与汇合
```

## 相关项目

- [`libc`](https://github.com/BRX-Boruix/libc) —— 提供错误码与线程局部存储的管理
- [`libsys`](https://github.com/BRX-Boruix/libsys) —— 提供线程派生、汇合与内存映射接口
- [`init`](https://github.com/BRX-Boruix/init) —— 在启动序列中拉起本程序

## 许可

MIT License，版权归 Yang Borui 所有。详见 [LICENSE](LICENSE)。

---

# English

[简体中文](#threaddemo) | **English**

A **user-space multithreading example** for BORUIX — it demonstrates and verifies how multiple threads
inside one process really behave.

It spawns two threads in a process, lets them run on **separate stacks** while sharing one address
space, gives each its own **private thread-local data**, and finally joins them.

```
[threaddemo] thread-a tid=... running
[threaddemo] thread-b tid=... running
[threaddemo] PASS (2 threads joined)
```

---

## What it verifies

Threads are not a single isolated feature; four things must hold at once:

| Verified | Meaning |
| --- | --- |
| **A shared address space** | Threads of one process see the **same** globals and code |
| **Separate stacks** | Each thread has its own stack; they do not overwrite each other |
| **Private thread-local storage** | Each thread's local data is **untouched by the other**, and stays its own across switches |
| **Thread identity** | Each thread has its own queryable identity and can find the process it belongs to |

The first two are intuitively clear; the latter two are where mistakes happen, and are treated
separately below.

## Shared versus separate

The essence of a thread versus a process is **what is shared and what is not**. This example
demonstrates both at once:

- The two threads accumulate **the same** global counter, and the total comes out right — proof the
  address space is shared
- Each thread runs on its **own stack** — proof stacks are not shared

Were the stack wrongly shared, the threads would corrupt each other's call frames and the program
would crash or interleave its output during the run. So "one stack per thread" must be allocated
first, with the stack top handed to the spawn interface.

## Thread-local data: where it goes wrong

Each thread needs **its own** copy of certain data — the error code being the classic example. Were
two threads to share one error-code variable, the value thread A set would be overwritten by thread
B, and A would read someone else's error.

This example verifies precisely that, and **adversarially**:

1. The two threads set **different** error codes
2. Each deliberately yields the CPU (forcing a scheduling switch)
3. On waking, each reads its own value back

**Each also reads the other's**: if your own value is right but the other's equals yours, the two are
really using one copy of storage — they merely happened to hold the same value. Checking both
directions is what rules out the illusion of correctness.

Thread-local storage is verified the same way: each thread allocates a slot from **its own** area and
writes a unique marker, which must still be its own after a scheduling switch. It further checks that
the two areas **do not overlap** — were the allocator to hand the same block to both threads, the
markers would clobber each other.

## The thread entry convention

This is the example's most important technical point, and the easiest to trip over.

Starting a new thread is **not the same as calling a function**. A function call pushes a return
address onto the stack so the function can jump back; a new thread's stack is **brand new** and holds
**no return address** — there is nothing to return to.

A thread entry must therefore satisfy two requirements:

| Requirement | Reason |
| --- | --- |
| **Never return normally** | With no return address on the stack, returning jumps to an unknown location |
| **Handle stack alignment itself** | There is no caller to prepare a stack frame for you |

The example's approach: the entry is a very thin assembly shell that first aligns the stack, then
calls a body function that never returns (it ends by terminating the thread). The body can then be
written like ordinary code, without caring that it was started in an unusual way.

**A note for anyone writing a similar program**: if your thread entry tries to `return` normally, the
program jumps to an invalid address. It is the most common failure point in this interface.

## Thread identity

Every thread can query its own identity and can find the **process it belongs to**.

The two values must **differ**: the identity is unique among threads, while the owning process is the
**same** for all of them. An implementation that conflates the two breaks every decision made on
"which process does this belong to" — signal delivery and resource ownership among them.

The example checks three things: the main thread's identity matches its process (it is the only
thread), the two child threads have distinct identities different from the main thread's, and both
child threads report **exactly the same** owning process.

## How it runs

It is started once by the system init process during the boot sequence, which waits for it and
collects its exit code.

**It is a one-shot verification load**: it runs and exits, and does not stay resident.

## Output and verdict

Each thread prints its own progress, then the join result is printed.

Success is the line `PASS (2 threads joined)` with a zero exit. Any failed check prints the specific
failing item and returns a non-zero exit code.

## Building

```bash
cargo build --release
```

The artifact is deployed as a user-space program in a BORUIX system.

## Layout

```
threaddemo/
├── Cargo.toml    # package definition
├── build.rs      # injects the linker script
├── linker.ld     # user-space section layout
└── src/
    └── main.rs   # spawning, verifying, and joining the two threads
```

## Related projects

- [`libc`](https://github.com/BRX-Boruix/libc) — provides error codes and thread-local storage management
- [`libsys`](https://github.com/BRX-Boruix/libsys) — provides thread spawn, join, and memory mapping interfaces
- [`init`](https://github.com/BRX-Boruix/init) — starts this program in the boot sequence

## License

MIT License, copyright Yang Borui. See [LICENSE](LICENSE).
