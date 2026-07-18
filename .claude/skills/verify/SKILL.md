---
name: verify
description: Build, run, and drive the Plato SDL emulator in a containerized session (no system SDL2/mupdf) to verify reader changes end-to-end with screenshots and synthetic taps.
---

# Verifying Plato changes with the emulator

Recipe for getting `plato-emulator` running in a Claude Code container and
driving it headlessly. Worked as of mupdf 1.27; adjust versions to whatever
`crates/core/src/document/mupdf_sys.rs` pins.

## 1. System packages

```sh
apt-get update -qq
apt-get install -y libsdl2-dev xdotool imagemagick xvfb \
  libdjvulibre-dev libharfbuzz-dev libfreetype-dev libjpeg-dev libpng-dev \
  libbz2-dev libgumbo-dev libjbig2dec0-dev libopenjp2-7-dev
```

## 2. MuPDF headers (for the wrapper)

The official `mupdf.com` tarball URL redirects to GitHub releases, which the
session proxy blocks for out-of-scope repos. Use the Ubuntu source package
instead (archive.ubuntu.com is reachable):

```sh
cd thirdparty
curl -sSo downloads/mupdf.orig.tar.xz \
  http://archive.ubuntu.com/ubuntu/pool/universe/m/mupdf/mupdf_1.27.0+ds1.orig.tar.xz
tar -xJ --strip-components 1 -C mupdf -f downloads/mupdf.orig.tar.xz
```

Only the headers are needed from this tree.

## 3. Shared libmupdf.so (from the PyMuPDF wheel)

Do NOT build libmupdf from the `+ds` repack: it strips the embedded noto/droid
fonts and plato links their `_binary_resources_fonts_*` symbols directly, so
the final link fails. The manylinux PyMuPDF wheel on PyPI
(files.pythonhosted.org is proxy-allowed) ships a complete shared library with
those symbols exported:

```sh
# pick the PyMuPDF release matching the pinned mupdf minor version
curl -sL -o pymupdf.whl "$(curl -s https://pypi.org/pypi/PyMuPDF/1.27.1/json | \
  python3 -c "import json,sys; d=json.load(sys.stdin); print([u['url'] for u in d['urls'] if 'manylinux' in u['filename'] and 'x86_64' in u['filename']][0])")"
python3 -m zipfile -e pymupdf.whl whl
cp whl/pymupdf/libmupdf.so.27.* /usr/local/lib/ && ldconfig
```

**Version check**: `fz_new_context` aborts if versions mismatch. Both the
`FZ_VERSION` constant in `crates/core/src/document/mupdf_sys.rs` and the one
in `thirdparty/mupdf/include/mupdf/fitz/version.h` must equal the `.so`'s
exact version (e.g. wheel 1.27.1 vs pinned 1.27.0). Bump them temporarily for
the test run and REVERT before committing.

## 4. Build

```sh
./scripts/host-libs.sh                 # dev symlinks in target/host-libs
(cd mupdf_wrapper && ./build.sh)       # needs the headers from step 2
touch crates/core/build.rs             # force re-link after lib swaps
cargo build -p emulator
```

## 5. Run headless and drive it

Settings.toml (gitignored, repo root) — point a library at a scratch dir; a
multi-chapter epub is easy to generate with python's `zipfile` (mimetype entry
stored uncompressed, OPF + NCX + a few xhtml chapters). Reader tap zones are
configured in the same file, e.g. `south-east-corner = "toggle-progress-bar"`.

```sh
Xvfb :99 -screen 0 1024x900x24 &
DISPLAY=:99 ./target/debug/plato-emulator > /tmp/emulator.log 2>&1 &
DISPLAY=:99 xdotool search --name "Plato Emulator" getwindowgeometry
DISPLAY=:99 xdotool mousemove <x> <y> click 1     # tap
DISPLAY=:99 import -window root shot.png          # screenshot
```

The default (unset PRODUCT env) device is a Kobo Touch: 600×800 at 167 dpi;
the window typically sits at (212,50) on a 1024×900 screen. With the default
`corner-width = 0.4`, corner taps land reliably ~5 px inside the corner
(SE ≈ window (595,795)); east/west strip taps at mid-height turn pages;
center taps toggle the bars.

## Gotchas

- `pkill -f plato-emulator` kills your own shell (the pattern matches the
  compound command); use `pkill -x plato-emulator`.
- Settings (including runtime-toggled ones) are saved to Settings.toml on
  clean exit/SIGTERM — handy for persistence checks, surprising otherwise.
- Epubs are rendered by plato's own HTML engine with the repo's bundled
  `fonts/`; mupdf is only exercised by PDF/DjVu and library thumbnails, so a
  fonts-less mupdf would still render epubs (but the link needs the symbols
  regardless, see step 3).
- Emulator stdout/stderr is quiet; panics land in the log file you redirected.
