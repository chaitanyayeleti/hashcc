# Maintainer: You <you@example.com>
_pkgname=hashsum
pkgname=${_pkgname}
pkgver=0.1.0
pkgrel=1
pkgdesc="Generate, compare, verify file hashes (SHA*, BLAKE3) with parallelism and sumfile compatibility"
arch=('x86_64' 'aarch64')
url="https://example.com/${_pkgname}"
license=('custom')
provides=(${_pkgname})
conflicts=(${_pkgname}-git)
depends=()
makedepends=('rust' 'cargo')
options=(!debug)
# To enable optional features, set FEATURES env before makepkg, e.g. FEATURES="progress,archives"
# Build directly in startdir (the directory containing PKGBUILD and sources)

build() {
  cd "${startdir}"
  # Use FEATURES env to opt-in to cargo features
  # CARGO_TARGET_DIR points to makepkg's srcdir to isolate build artifacts
  export CARGO_TARGET_DIR="${srcdir}/cargo-target"
  cargo build --release ${FEATURES:+--features "$FEATURES"}
}

check() {
  : # No tests
}

package() {
  cd "${startdir}"
  install -Dm755 "${srcdir}/cargo-target/release/${_pkgname}" "${pkgdir}/usr/bin/${_pkgname}"
  # If you add a LICENSE file later, install it like below
  # install -Dm644 LICENSE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
}
