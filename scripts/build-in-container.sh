#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

podman build -t plato-build .
podman run --rm \
  -v "$PWD:/workspace:Z" \
  -v plato-cargo-registry:/usr/local/cargo/registry \
  -v plato-cargo-git:/usr/local/cargo/git \
  plato-build \
  sh -c './build.sh slow && ./dist.sh'
