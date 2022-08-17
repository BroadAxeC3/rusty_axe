## Introduction

Some times you need to keep a little bit of log data for debugging purposes
or perhaps you need to document why an EC2 instance keeps crashing.  This program takes
a file and shoves it into CWLogs as quickly as possible, with as little fuss as possible.

## Build

### Requirements
* Rust
* Docker || Podman (NOTE: Podman currently doesn't work on macOS - [issue #757](https://github.com/cross-rs/cross/issues/757))

### Steps
1. Install the "cross" command: `cargo install -f cross`
1. Use "cross" to build (force Podman): `CROSS_CONTAINER_ENGINE=podman cross build --target aarch64-unknown-linux-gnu`
