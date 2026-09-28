# threaddemo

BORUIX 的多线程验收程序：同进程双线程的创建、回收、线程独立 errno 与线程本地存储。

[English](README.en.md)

## 测什么

主线程创建两个子线程，三者共享地址空间，各自有独立的栈、线程控制块与 TLS 区域：

- 两线程对共享计数器各累加 5 次，合计 10
- 每线程设置自己的 errno 并在多次让出后回读：线程 a 恒为 2，线程 b 恒为 11，互不串扰
- 每线程从自己的 TLS 区申请槽位写入标记，跨让出回读不变，两个区域与槽位地址互不重叠
- 组长的进程号与线程号相等，两个成员的进程号等于组长线程号、线程号各异

全部满足时输出：

```
[threaddemo] threaddemo PASS (2 threads joined, per-thread errno + TLS + gettid/getpid identity)
```

## 退出码

- `0`——全部通过
- `2` 到 `5`——栈或线程控制块的内存分配失败
- `8`、`9`——线程回收失败
- `10`——判定失败（计数、回收码、errno、TLS 或身份不符，输出行会指出哪一项）

## 构建

```bash
cargo build --release
```

## 文件结构

```
threaddemo/
├── Cargo.toml    # 包定义
├── build.rs      # 注入链接脚本
├── linker.ld     # 用户态段布局
└── src/
    └── main.rs   # 线程体、TLS 检查与判定
```

## 相关项目

- [`libsys`](https://github.com/BRX-Boruix/libsys) —— 线程创建与回收接口
- [`libc`](https://github.com/BRX-Boruix/libc) —— 线程本地存储与 errno

## 许可

MIT License，版权归 Yang Borui 所有。详见 [LICENSE](LICENSE)。
