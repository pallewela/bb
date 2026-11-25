#!/usr/bin/env bash
# Render the AUR package bitbucket-bb-cli-bin for one release:
#   packaging/aur/render.sh VERSION SHA256_X86_64 SHA256_AARCH64 OUTDIR
# writes OUTDIR/PKGBUILD and OUTDIR/.SRCINFO. The checksums are those of the release's
# bb-vVERSION-{x86_64,aarch64}-unknown-linux-musl.tar.gz. .SRCINFO is written here instead of
# with `makepkg --printsrcinfo`, so this runs anywhere (CI has no Arch); keep it in step with
# PKGBUILD.in.
set -euo pipefail

if [ $# -ne 4 ]; then
  echo "usage: $0 VERSION SHA256_X86_64 SHA256_AARCH64 OUTDIR" >&2
  exit 2
fi
VERSION=$1 X86=$2 ARM=$3 OUT=$4
printf '%s\n' "$VERSION" | grep -qxE '[0-9]+\.[0-9]+\.[0-9]+' || { echo "bad version: $VERSION" >&2; exit 2; }
for s in "$X86" "$ARM"; do
  printf '%s\n' "$s" | grep -qxE '[0-9a-f]{64}' || { echo "bad sha256: $s" >&2; exit 2; }
done

HERE=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$OUT"
sed -e "s/@VERSION@/${VERSION}/g" -e "s/@SHA256_X86_64@/${X86}/g" -e "s/@SHA256_AARCH64@/${ARM}/g" \
  "$HERE/PKGBUILD.in" > "$OUT/PKGBUILD"

URL=https://github.com/pallewela/bb/releases/download/v${VERSION}
cat > "$OUT/.SRCINFO" <<EOF
pkgbase = bitbucket-bb-cli-bin
	pkgdesc = gh-style CLI for Bitbucket Cloud (pull requests, Pipelines, repos, API)
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/pallewela/bb
	arch = x86_64
	arch = aarch64
	license = MIT
	optdepends = xdg-utils: open pages in the browser with --web
	provides = bitbucket-bb-cli
	conflicts = bitbucket-bb-cli
	options = !strip
	options = !debug
	source_x86_64 = ${URL}/bb-v${VERSION}-x86_64-unknown-linux-musl.tar.gz
	sha256sums_x86_64 = ${X86}
	source_aarch64 = ${URL}/bb-v${VERSION}-aarch64-unknown-linux-musl.tar.gz
	sha256sums_aarch64 = ${ARM}

pkgname = bitbucket-bb-cli-bin
EOF
echo "Rendered bitbucket-bb-cli-bin ${VERSION} into ${OUT}"
