#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

# crosstool-ng needs more than 1024 open files; don't depend on the host's
# hard limit allowing it to raise the soft limit itself.
podman build --ulimit nofile=2048:2048 -t plato-build .
podman run --rm \
  --ulimit nofile=2048:2048 \
  -v "$PWD:/workspace:Z" \
  -v plato-cargo-registry:/usr/local/cargo/registry \
  -v plato-cargo-git:/usr/local/cargo/git \
  plato-build \
  sh -c './build.sh slow && ./dist.sh'
