#! /bin/sh

[ -d build ] && rm -Rf build

mkdir build
cd build || exit 1

TRIPLE=arm-linux-gnueabihf
# -ffast-math (added by openjpeg's CMake) makes old glibc's math-finite.h
# call lgamma_r without a declaration; GCC >= 14 treats that as an error.
export CFLAGS="-O2 -mcpu=cortex-a9 -mfpu=neon -Wno-error=implicit-function-declaration"
export CXXFLAGS="$CFLAGS"

cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_CODEC=off -DBUILD_STATIC_LIBS=off -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_C_COMPILER=${TRIPLE}-gcc -DCMAKE_CXX_COMPILER=${TRIPLE}-g++ -DCMAKE_AR=${TRIPLE}-ar .. && make

cd .. || exit 1
cp build/src/lib/openjp2/opj_config.h src/lib/openjp2
