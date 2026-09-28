# threaddemo

An **end-to-end multithread acceptance program** for BORUIX, verifying that when several threads are spawned within one process the address space, stacks, error codes, and thread-local storage are each independent and correct.

[简体中文](README.md)

## What it tests

The program spawns two threads in the same process and then verifies each item:

| Item | Contents |
| --- | --- |
| **Shared address space** | Both threads share one address space — concurrent accumulation into a shared counter reaches the expected value |
| **Independent stacks** | Each thread has its own user stack (each separately mapped) |
| **Joining and exit codes** | The leader joins both threads and each exit code is correct |
| **Per-thread error codes** | Thread A sets "no such file" (2), thread B sets "try again later" (11); **read back repeatedly across yields**, each thread must always see its own value |
| **Thread-local storage** | Each thread allocates a slot from **its own** arena and writes a unique marker; the marker must survive across yields, and the two arenas and slot pointers must be **disjoint** |
| **Thread identity** | Thread IDs differ, and the process ID is the same |

## The key design: how error codes and local storage become per-thread

Each thread installs its own **thread control block** and points a segment base register at it (the kernel saves and restores that register on every switch). Error code access and thread-local storage addressing both go through that register, so they land naturally on **the current thread's own control block**.

That yields a direct failure mode: **if the segment base leaks across a switch** (not properly restored), thread B reads thread A's slot. The acceptance targets exactly this — which is why it requires repeated reads across yields rather than a single read.

## Usage

Run it standalone; no arguments.

```
threaddemo PASS (2 threads joined, per-thread errno + TLS + gettid/getpid identity)
```

## Exit codes

| Exit code | Meaning |
| --- | --- |
| `0` | Everything passed |
| Non-zero | Failure |

## Building

```bash
cargo build --release
```

## Layout

```
threaddemo/
├── Cargo.toml    # package definition
├── build.rs      # injects the linker script
├── linker.ld     # user-space section layout
└── src/
    └── main.rs   # spawning the two threads and verifying each item
```

## Related projects

- [`libc`](https://github.com/BRX-Boruix/libc) — provides the thread control block and thread-local storage
- [`libsys`](https://github.com/BRX-Boruix/libsys) — provides thread spawn, join, and yield interfaces
- [`selftest`](https://github.com/BRX-Boruix/selftest) — the self-test host for the thread group

## License

MIT License, copyright Yang Borui. See [LICENSE](LICENSE).
