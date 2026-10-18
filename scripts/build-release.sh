#!/usr/bin/env bash
#
# build-release.sh — build the Linux release on the Ubuntu box.
#
# The Windows release is scripts/build-release.ps1; this is its Linux sibling,
# and it is deliberately smaller. There is no Authenticode here (it signs
# Windows binaries only) and no updater signature (the updater feed serves the
# Windows NSIS installer; a deb or rpm is updated by its package manager, and
# Flathub updates the flatpak). What remains is the build itself, from a
# committed tree.
#
# createUpdaterArtifacts is turned off for the build through a config
# override. The base config enables it for the Windows updater, and the deb
# bundler treats a deb as an updater target too, so with it left on the build
# stops with "A public key has been found, but no private key" — this machine
# does not have, and must not have, the Windows updater key. The slice-0 CI
# job hit exactly this and passes --no-sign; a release takes the override
# instead, so the artifact that ships is the artifact the release process
# describes. Signing stays on the Windows release path it belongs to.
#
# The rpm dependency names in src-tauri/tauri.linux.conf.json are the
# Fedora-family names best known when they were written, and they are not
# verified. The first rpm build must check them on a Fedora-family machine
# (`rpm -qpR` on the built package) and correct the config where they differ.
#
# This script has not been run yet: it is written for the Ubuntu session,
# which runs it first. It refuses a dirty worktree and asserts its tools
# before starting, because both failures are clearer now than fifteen minutes
# into a build.
#
# Usage (from the repository root):
#
#   ./scripts/build-release.sh

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  printf 'build-release: %s\n' "$1" >&2
  exit 1
}

if [[ -n "$(git status --porcelain)" ]]; then
  fail "the worktree is dirty. A release is built from a committed tree only."
fi

for tool in bun cargo git; do
  command -v "$tool" >/dev/null 2>&1 || fail "$tool is not on PATH."
done

# Stale bundles from an earlier build must never be mistaken for this one's.
rm -rf src-tauri/target/release/bundle/deb src-tauri/target/release/bundle/rpm

config_path="src-tauri/target/bundle-config.json"
trap 'rm -f "$config_path"' EXIT
mkdir -p "$(dirname "$config_path")"
printf '%s\n' '{"bundle":{"createUpdaterArtifacts":false}}' >"$config_path"

bun run tauri build --bundles deb,rpm --config "$config_path"

shopt -s nullglob
artifacts=(src-tauri/target/release/bundle/deb/*.deb src-tauri/target/release/bundle/rpm/*.rpm)
if (( ${#artifacts[@]} == 0 )); then
  fail "the build reported success but neither a deb nor an rpm exists."
fi

echo
echo "Artifacts:"
for artifact in "${artifacts[@]}"; do
  printf '  %s\n' "$artifact"
  printf '    %s bytes\n' "$(stat -c %s "$artifact")"
  printf '    sha256 %s\n' "$(sha256sum "$artifact" | cut -d' ' -f1)"
done
