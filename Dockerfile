# koxtoolchain's CI builds on ubuntu-20.04; newer texinfo (7.x, in 24.04)
# breaks the old glibc's documentation build.
FROM ubuntu:20.04 AS toolchain

ENV DEBIAN_FRONTEND=noninteractive

# Dependency list from koxtoolchain's README (Debian/Ubuntu section).
RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf \
    automake \
    autotools-dev \
    bison \
    build-essential \
    ca-certificates \
    curl \
    file \
    flex \
    gawk \
    git \
    gperf \
    help2man \
    libncurses-dev \
    libtool \
    libtool-bin \
    texinfo \
    unzip \
    wget \
    xz-utils \
 && rm -rf /var/lib/apt/lists/*

# GNU Savannah (config.sub) and other download hosts intermittently return
# server errors; shadow wget with a retrying wrapper for all ct-ng downloads.
RUN printf '#!/bin/sh\nexec /usr/bin/wget --tries=10 --waitretry=10 --timeout=30 --retry-on-http-error=429,500,502,503,504 "$@"\n' \
      > /usr/local/bin/wget \
 && chmod +x /usr/local/bin/wget

# crosstool-ng refuses to build as root.
RUN useradd -m tc
USER tc
WORKDIR /home/tc

ARG KOXTOOLCHAIN_REF=2024.10
RUN git clone https://github.com/NiLuJe/koxtoolchain.git \
 && cd koxtoolchain \
 && git checkout "${KOXTOOLCHAIN_REF}" \
 # 'ct-ng updatetools' fetches config.{sub,guess} from GNU Savannah, which
 # fails intermittently; use the copies shipped by autotools-dev instead.
 # Also lower glibc's minimum kernel from the sample's 4.1 to 3.2.0: Kobo
 # kernels never update with firmware, and the historical Linaro-built
 # artifacts carried a 3.2.0 floor, so 4.1 would drop older devices.
 && sed -i 's|ct-ng updatetools|mkdir -p scripts \&\& cp /usr/share/misc/config.sub /usr/share/misc/config.guess scripts/ \&\& sed -i "s#^CT_GLIBC_MIN_KERNEL_VERSION=.*#CT_GLIBC_MIN_KERNEL_VERSION=\\"3.2.0\\"#" .config \&\& ct-ng olddefconfig|' gen-tc.sh \
 && grep -q 'config.sub' gen-tc.sh \
 && ./gen-tc.sh kobov4 \
 # Only x-tools is needed downstream; the ct-ng build tree is many GB and
 # would otherwise bloat the committed layer past available disk.
 && rm -rf /home/tc/koxtoolchain

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

COPY --from=toolchain /home/tc/x-tools/arm-kobov4-linux-gnueabihf /opt/x-tools/arm-kobov4-linux-gnueabihf

# The build scripts expect the arm-linux-gnueabihf- prefix; the x-tools tree
# is read-only, so put the renamed symlinks in /usr/local/bin instead.
RUN for f in /opt/x-tools/arm-kobov4-linux-gnueabihf/bin/arm-kobov4-linux-gnueabihf-*; do \
      ln -s "$f" "/usr/local/bin/arm-linux-gnueabihf-${f##*/arm-kobov4-linux-gnueabihf-}"; \
    done

ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/opt/x-tools/arm-kobov4-linux-gnueabihf/bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init.sh https://sh.rustup.rs \
 && sh /tmp/rustup-init.sh -y --default-toolchain stable --profile minimal --target arm-unknown-linux-gnueabihf \
 && rm /tmp/rustup-init.sh

WORKDIR /workspace
