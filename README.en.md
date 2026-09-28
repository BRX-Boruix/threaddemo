# threaddemo

A BORUIX multithread acceptance test: same-process two-thread creation, reaping, per-thread errno and thread-local storage.

[简体中文](README.md)

## What it tests

The leader spawns two child threads sharing one address space, each with its own stack, thread control block and TLS arena:

- Both threads add 5 to a shared counter, 10 in total
- Each thread sets its own errno and re-reads it across yields: thread a stays 2, thread b stays 11, no cross-talk
- Each thread allocates TLS slots from its own arena, writes markers, and the markers survive yields; the two arenas and slot addresses stay disjoint
- The leader's pid equals its tid; both members report the leader tid as their pid and distinct tids

On success it prints:

```
[threaddemo] threaddemo PASS (2 threads joined, per-thread errno + TLS + gettid/getpid identity)
```

## Exit codes

- `0` — all checks passed
- `2` to `5` — memory allocation failure for a stack or thread control block
- `8`, `9` — thread reaping failed
- `10` — a check failed (counter, join codes, errno, TLS or identity; the output lines say which)

## Building

```bash
cargo build --release
```

## Repository layout

```
threaddemo/
├── Cargo.toml    # package manifest
├── build.rs      # injects the linker script
├── linker.ld     # user-space segment layout
└── src/
    └── main.rs   # thread bodies, TLS checks, verdict
```

## Related projects

- [`libsys`](https://github.com/BRX-Boruix/libsys) — thread spawn and join interfaces
- [`libc`](https://github.com/BRX-Boruix/libc) — thread-local storage and errno

## License

MIT License, copyright Yang Borui. See [LICENSE](LICENSE).
