#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

# Tag the image by Dockerfile hash (as CI does) so an unchanged Dockerfile
# reuses the already-built image instead of rebuilding; only a Dockerfile
# change forces a rebuild. Building from scratch compiles the cross toolchain
# and takes ~an hour.
ref="plato-build:$(sha256sum Dockerfile | cut -c1-16)"
if podman image exists "$ref"; then
  echo "Reusing existing build image $ref"
  podman tag "$ref" plato-build
else
  echo "No image for the current Dockerfile; building $ref"
  # crosstool-ng needs more than 1024 open files; don't depend on the host's
  # hard limit allowing it to raise the soft limit itself.
  podman build --ulimit nofile=2048:2048 -t plato-build -t "$ref" .
fi

# The workspace is bind-mounted, so libs/ and the built thirdparty trees persist
# between runs. After the first build, reuse the compiled C dependencies and go
# straight to the wrapper + Rust build; a full C rebuild happens only when the
# artifacts are gone. Delete them (rm -rf libs) to force one, e.g. after bumping
# a dependency version in thirdparty/download.sh. The stamp is written inside the
# container only after build.sh succeeds, so a failed dep build leaves no false
# marker and a dist.sh failure doesn't force a needless C rebuild next time.
if [ -e libs/.deps-built ]; then
  echo "Reusing built C dependencies (rm -rf libs to force a rebuild)"
  build_step='./build.sh skip'
else
  echo "Building C dependencies from scratch"
  build_step='./build.sh slow'
fi

podman run --rm \
  --ulimit nofile=2048:2048 \
  -v "$PWD:/workspace:Z" \
  -v plato-cargo-registry:/usr/local/cargo/registry \
  -v plato-cargo-git:/usr/local/cargo/git \
  plato-build \
  sh -c "$build_step && touch libs/.deps-built && ./dist.sh"
