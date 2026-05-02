FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf \
    automake \
    build-essential \
    ca-certificates \
    cmake \
    curl \
    git \
    jq \
    libtool \
    meson \
    ninja-build \
    patchelf \
    pkg-config \
    python3 \
    unzip \
    wget \
    xz-utils \
 && rm -rf /var/lib/apt/lists/*

ARG LINARO_REL=5.5-2017.10
ARG LINARO_VER=5.5.0-2017.10
RUN tarball="gcc-linaro-${LINARO_VER}-x86_64_arm-linux-gnueabihf.tar.xz" \
 && wget -q "https://releases.linaro.org/components/toolchain/binaries/${LINARO_REL}/arm-linux-gnueabihf/${tarball}" \
 && mkdir -p /opt/linaro \
 && tar -xf "${tarball}" -C /opt/linaro --strip-components=1 \
 && rm "${tarball}"

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/opt/linaro/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init.sh https://sh.rustup.rs \
 && sh /tmp/rustup-init.sh -y --default-toolchain stable --profile minimal --target arm-unknown-linux-gnueabihf \
 && rm /tmp/rustup-init.sh

WORKDIR /workspace
