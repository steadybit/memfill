# memfill

`memfill` is a small utility that fills up memory for a configured duration. It is used to reproduce memory pressure scenarios during reliability experiments.

## Usage

```
memfill <size> <alloc-mode> <duration> [--reserve <size>] [--adaptive] [--oom-score-adj <n>] [--ignore-cgroup]
        [--target-cgroup-path <path>] [--target-pid <pid>]

ARGS:
    <size>          Size of memory to fill up; suffixes: K, M, G or %
    <alloc-mode>    Allocation mode; [absolute, usage]
    <duration>      Duration; suffixes: s, m, h, d

OPTIONS:
    --reserve <size>       Minimum memory to keep available (reserve); suffixes: K, M, G or %.
                           In usage mode it floors the memory left free; in absolute mode it caps
                           the allocation so at least this much stays free. Use it to fill a host
                           hard without starving the OS/kubelet (which would make a node NotReady).
    --oom-score-adj <n>    (Linux only) oom_score_adj for the fill process (-1000..1000). Defaults
                           to -1000 when privileged, 0 otherwise. For host fills pass a high value
                           so the OOM killer targets the fill first, not critical processes.

    --target-cgroup-path <path>
                           (Linux only) Join this cgroup before allocating, so the fill is charged
                           against the target's memory limit instead of memfill's own. A plain path
                           fragment (e.g. /kubepods/besteffort/pod123/container456), no controller
                           prefix. Requires root, and a view of the host's /sys/fs/cgroup.
    --target-pid <pid>     (Linux only) Fork the allocation processes into this PID's namespace, so
                           they show up in the target container's own `ps`/`top`. Uses
                           setns(CLONE_NEWPID), which by design affects only children forked
                           afterward - memfill itself stays in its original namespace. Requires root.

FLAGS:
    --adaptive        (Linux only) Free memory when the host is under memory pressure (PSI),
                      keeping it responsive. Requires usage mode.
    --ignore-cgroup   (Linux only) Ignore cgroup limits; compute total/usage from system information instead
    -h, --help        Print help information
    -V, --version     Print version information
```

### Examples

Fill 500 MB of memory in absolute mode for 30 seconds:

```bash
memfill 500M absolute 30s
```

Fill memory up to 80% usage for five minutes:

```bash
memfill 80% usage 5m
```

Fill a host to 100% usage but always leave 512 MiB free, so the node's OS/kubelet stay alive:

```bash
memfill 100% usage 2m --reserve 512MiB
```

Same, but also back off automatically when the host starts thrashing (PSI), for maximum survivable pressure:

```bash
memfill 100% usage 2m --reserve 512MiB --adaptive
```

Fill a *different* container's memory, scoped to its cgroup limit and visible in its own `ps`
(this is how the Steadybit extensions run the fill-memory attack against a target container):

```bash
memfill 80% usage 1m \
  --target-cgroup-path /kubepods/besteffort/pod123/container456 \
  --target-pid 4242
```

## Requirements

- Linux or Windows (`x86_64`, `aarch64` on Linux)
- No special privileges required for the default mode; reading cgroup memory limits is the only OS-level interaction
- `--target-cgroup-path`/`--target-pid` do require root (CAP_SYS_ADMIN for `setns`, write access to the
  target's `cgroup.procs`) and a view of the host's `/sys/fs/cgroup` and `/proc`

## Building

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable; toolchain is pinned via `rust-toolchain.toml`)
- [cross](https://github.com/cross-rs/cross) for cross-compilation
- [Docker](https://www.docker.com/) (required by `cross` to run the build containers)
- [GNU Make](https://www.gnu.org/software/make/)

### Build

```bash
make build
```

Cross-compiles release binaries for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and `x86_64-pc-windows-gnu`. The artefacts are placed under `target/<triple>/release/`.

### Run Tests

```bash
cargo test
```

Integration tests under `tests/` use [testcontainers](https://testcontainers.com/), so a working Docker installation is required.

## License

MIT - see [LICENSE](LICENSE).
