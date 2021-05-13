ffav-sys
========

[![ffav-sys on crates.io](https://img.shields.io/crates/v/ffav-sys?cacheSeconds=3600)](https://crates.io/crates/ffav-sys)
[![Build Status](https://ci.vaxpl.com/api/badges/rdst/ffav-sys/status.svg?ref=refs/heads/{{BRANCH_NAME}})](https://ci.vaxpl.com/rdst/ffav-sys)

This is a fork of the abandoned [ffmpeg-sys-next](https://github.com/zmwangx/rust-ffmpeg-sys) crate. You can find this crate as [ffav-sys](https://crates.io/crates/ffav-sys) on crates.io.

This crate contains low level bindings to FFmpeg. You're probably interested in the high level bindings instead: [ffav-rs](https://github.com/vaxpl/ffav-rs).

A word on versioning: major and minor versions track major and minor versions of FFmpeg, e.g. 4.2.x of this crate has been updated to support the 4.2.x series of FFmpeg. Patch level is reserved for bug fixes of this crate and does not track FFmpeg patch versions.

FAQ
===

Cross Compilation
-----------------

To build with cross toolchain, you shoud be set `BINDGEN_EXTRA_CLANG_ARGS`
to tell the `clang` where to find the headers, if not do this, the `bindgen` maybe raise an error like:

```text
/usr/include/gnu/stubs.h:7:11: fatal error: 'gnu/stubs-32.h' file not found
/usr/include/gnu/stubs.h:7:11: fatal error: 'gnu/stubs-32.h' file not found, err: true
thread 'main' panicked at 'Unable to generate baldrapi.h bindings: ()', src/libcore/result.rs:1009:5
```

For example, to build with `--target=aarch64-unknown-linux-gnu --features=bundled`:

```toml
# .cargo/config.toml:
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

```sh
# Shell commands:
export BINDGEN_EXTRA_CLANG_ARGS="--sysroot=/usr/aarch64-linux-gnu"
cargo build --target=aarch64-unknown-linux-gnu --features=bundled
```
