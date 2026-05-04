#! /bin/sh

[ -d dist ] && rm -Rf dist

[ -d bin ] || ./download.sh 'bin/*'
[ -d resources ] || ./download.sh 'resources/*'
[ -d hyphenation-patterns ] || ./download.sh 'hyphenation-patterns/*'
[ -e target/arm-unknown-linux-gnueabihf/release/plato ] || ./build.sh

mkdir -p dist/libs
mkdir dist/dictionaries

copy_with_soname() {
	src=$1
	dest_dir=$2
	soname=$(arm-linux-gnueabihf-readelf -d "$src" | awk '/SONAME/ { gsub(/[][]/, "", $NF); print $NF }')
	[ -n "$soname" ] || soname=$(basename "$src")
	cp "$src" "$dest_dir/$soname"
}

copy_with_soname libs/libz.so dist/libs
copy_with_soname libs/libbz2.so dist/libs

copy_with_soname libs/libpng16.so dist/libs
copy_with_soname libs/libjpeg.so dist/libs
copy_with_soname libs/libopenjp2.so dist/libs
copy_with_soname libs/libjbig2dec.so dist/libs

copy_with_soname libs/libfreetype.so dist/libs
copy_with_soname libs/libharfbuzz.so dist/libs

copy_with_soname libs/libgumbo.so dist/libs
copy_with_soname libs/libdjvulibre.so dist/libs
copy_with_soname libs/libmupdf.so dist/libs

cp -R hyphenation-patterns dist
cp -R keyboard-layouts dist
cp -R bin dist
cp -R scripts dist
cp -R icons dist
cp -R resources dist
cp -R fonts dist
cp -R css dist
find dist/css -name '*-user.css' -delete
find dist/keyboard-layouts -name '*-user.json' -delete
find dist/hyphenation-patterns -name '*.bounds' -delete
find dist/scripts -name 'wifi-*-*.sh' -delete
cp target/arm-unknown-linux-gnueabihf/release/plato dist/
cp contrib/*.sh dist
cp contrib/Settings-sample.toml dist
cp LICENSE-AGPLv3 dist

patchelf --remove-rpath dist/libs/*

arm-linux-gnueabihf-strip dist/plato dist/libs/*
