# memfill

`memfill` is a small utility that fills up memory for a configured duration. It is used to reproduce memory pressure scenarios during reliability experiments.

## Usage

```
memfill <size> <alloc-mode> <duration> [--ignore-cgroup]

ARGS:
    <size>          Size of memory to fill up; suffixes: K, M, G or %
    <alloc-mode>    Allocation mode; [absolute, usage]
    <duration>      Duration; suffixes: s, m, h, d

FLAGS:
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

## Requirements

- Linux or Windows (`x86_64`, `aarch64` on Linux)
- No special privileges required for the default mode; reading cgroup memory limits is the only OS-level interaction

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
