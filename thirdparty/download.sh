#! /usr/bin/env bash

set -e

declare -A urls=(
	# Compression
	# zlib.net rejects non-browser clients (HTTP 415); use the official GitHub mirror.
	["zlib"]="https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz"
	["bzip2"]="https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz"
	# Images
	["libpng"]="https://download.sourceforge.net/libpng/libpng-1.6.53.tar.gz"
	["libjpeg"]="http://www.ijg.org/files/jpegsrc.v9f.tar.gz"
	["openjpeg"]="https://github.com/uclouvain/openjpeg/archive/v2.5.4.tar.gz"
	["jbig2dec"]="https://github.com/ArtifexSoftware/jbig2dec/releases/download/0.20/jbig2dec-0.20.tar.gz"
	# Fonts
	["freetype2"]="https://download.savannah.gnu.org/releases/freetype/freetype-2.14.1.tar.gz"
	["harfbuzz"]="https://github.com/harfbuzz/harfbuzz/archive/12.3.0.tar.gz"
	# Documents
	["gumbo"]="https://github.com/google/gumbo-parser/archive/v0.10.1.tar.gz"
	["djvulibre"]="http://downloads.sourceforge.net/djvu/djvulibre-3.5.29.tar.gz"
	["mupdf"]="https://casper.mupdf.com/downloads/archive/mupdf-1.27.0-source.tar.gz"
)

# Downloaded tarballs are kept here so repeat builds (and CI cache restores)
# don't re-fetch them from upstream mirrors, some of which are flaky.
downloads="downloads"
mkdir -p "$downloads"

for name in "${@:-${!urls[@]}}" ; do
	url="${urls[$name]}"
	if [ ! "$url" ] ; then
		echo "Unknown library: ${name}." 1>&2
		exit 1
	fi
	# Cache each tarball as "<pkg>-<url-basename>": the basename carries the
	# version (so a bump fetches a fresh file), and the package prefix keeps
	# generic names like GitHub's v1.2.3.tar.gz from colliding across packages.
	tarball="${downloads}/${name}-$(basename "$url")"
	if [ -s "$tarball" ] ; then
		echo "Using cached ${name} (${tarball##*/})."
	else
		echo "Downloading ${name}."
		# Fetch to a temp file and publish it only on success so an interrupted
		# transfer never leaves a corrupt tarball in the cache. Retry to ride
		# out flaky mirrors (e.g. download.savannah.gnu.org).
		wget -q --show-progress \
			--tries=5 --retry-connrefused --waitretry=5 --timeout=30 \
			-O "${tarball}.part" "$url"
		mv "${tarball}.part" "$tarball"
	fi
	if [ -d "$name" ]; then
		git ls-files -o --directory -z "$name" | xargs -0 rm -rf
	else
		mkdir "$name"
	fi
	tar -xz --strip-components 1 -C "$name" -f "$tarball"
done
