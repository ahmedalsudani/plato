#! /bin/sh

# Populate target/host-libs with the linker dev names (libNAME.so) that the
# native emulator build expects, pointing at the versioned shared libraries
# already installed on the host. This avoids depending on the distribution's
# -devel packages just to get the unversioned symlinks.
#
# Re-run this whenever a system library is upgraded to a new soname version.

set -e

cd "$(dirname "$0")/.."

OUT=target/host-libs
mkdir -p "$OUT"

# Map each linker name (used as -lNAME) to the ldconfig prefix of the
# versioned library that provides it. SDL2 is shipped as libSDL2-2.0.so.*.
map="
mupdf     libmupdf.so
SDL2      libSDL2-2.0.so
freetype  libfreetype.so
harfbuzz  libharfbuzz.so
djvulibre libdjvulibre.so
png16     libpng16.so
jpeg      libjpeg.so
gumbo     libgumbo.so
openjp2   libopenjp2.so
jbig2dec  libjbig2dec.so
bz2       libbz2.so
"

echo "$map" | while read -r name prefix; do
	[ -z "$name" ] && continue
	path=$(ldconfig -p | awk -v p="$prefix" 'index($1, p) == 1 { print $NF; exit }')
	if [ -z "$path" ]; then
		printf 'host-libs: no system library found for -l%s (%s.*)\n' "$name" "$prefix" 1>&2
		exit 1
	fi
	ln -sf "$path" "$OUT/lib${name}.so"
	printf 'lib%s.so -> %s\n' "$name" "$path"
done

# MuPDF's third-party symbols come from the individual system libraries linked
# above, so -lmupdf-third only needs an empty stub to satisfy the linker.
echo '' | cc -shared -o "$OUT/libmupdf-third.so" -xc -
echo "libmupdf-third.so (empty stub)"
