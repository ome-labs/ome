#!/bin/sh
# Installs Ome: the `ome` command line, its daemon `omed` and `ome-keeper`.
#
#   curl -fsSL https://ome.network/install.sh | sh
#   curl -fsSL https://ome.network/install.sh | sh -s -- --version 0.1.0 --no-modify-path
#
# Installs into $OME_HOME (default ~/.ome): versions/<version>/, a `current` link to it, and bin/ links to its
# programs. Downloads the release archive for this platform from GitHub and checks it against the release's
# SHA256SUMS (and its GitHub attestation when `gh` is installed and signed in). The daemon needs Node 24 or later: the
# one on PATH if it is new enough, otherwise Node 24 from nodejs.org, checked against its SHASUMS256.txt and kept in
# $OME_HOME/runtime.
#
# Environment: OME_VERSION (instead of --version), OME_HOME, OME_DOWNLOAD_BASE (a URL or directory holding the
# archives and SHA256SUMS, instead of the GitHub release; for testing a build).
set -eu

REPO="ome-labs/ome"
NODE_MAJOR=24

say() { printf '%s\n' "$*"; }
fail() {
  printf 'ome install: %s\n' "$*" >&2
  exit 1
}
has() { command -v "$1" >/dev/null 2>&1; }

usage() {
  cat <<'EOF'
Installs Ome (https://ome.network).

  install.sh [--version <version>] [--no-modify-path]

  --version <version>   install this release (default: the latest; or set OME_VERSION)
  --no-modify-path      do not add ~/.ome/bin to PATH in your shell's profile
EOF
}

# Fetches a URL (or copies a local path) to a file.
fetch() {
  case $1 in
    http://* | https://*)
      if has curl; then
        curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
      elif has wget; then
        wget -q -O "$2" "$1"
      else
        fail "needs curl or wget"
      fi
      ;;
    file://*) cp "${1#file://}" "$2" ;;
    *) cp "$1" "$2" ;;
  esac
}

sha256() {
  if has sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif has shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    fail "needs sha256sum or shasum to check the download"
  fi
}

# Checks <file> against the line for <name> in <sums>.
verify() {
  expected=$(awk -v name="$2" '{ sub(/^\*/, "", $2) } $2 == name { print $1 }' "$3")
  [ -n "$expected" ] || fail "$2 is not listed in the checksums"
  actual=$(sha256 "$1")
  [ "$actual" = "$expected" ] || fail "$2 does not match its checksum (expected $expected, got $actual)"
}

detect_platform() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os/$arch" in
    Darwin/arm64) echo darwin-arm64 ;;
    Linux/x86_64 | Linux/amd64) echo linux-x64 ;;
    Linux/aarch64 | Linux/arm64) echo linux-arm64 ;;
    Darwin/x86_64)
      fail "Ome runs on Apple silicon Macs, not Intel ones. Supported: macOS arm64, Linux x64 and arm64."
      ;;
    *) fail "Ome has no build for $os $arch. Supported: macOS arm64, Linux x64 and arm64." ;;
  esac
}

latest_version() {
  fetch "https://api.github.com/repos/$REPO/releases/latest" "$work/latest.json" ||
    fail "could not ask GitHub for the latest release; pass --version"
  tag=$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$work/latest.json" | head -n 1)
  [ -n "$tag" ] || fail "could not read the latest release from GitHub; pass --version"
  echo "${tag#v}"
}

# A Node 24+ on PATH, or nothing.
system_node() {
  has node || return 0
  major=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
  if [ "$major" -ge "$NODE_MAJOR" ] 2>/dev/null; then
    command -v node
  fi
}

# Installs the latest Node $NODE_MAJOR into $OME_HOME/runtime and prints its path.
install_node() {
  base="https://nodejs.org/dist/latest-v$NODE_MAJOR.x"
  fetch "$base/SHASUMS256.txt" "$work/node-sums" || fail "could not download Node's checksums"
  file=$(awk -v suffix="-$1.tar.gz" 'index($2, suffix) && $2 ~ /^node-v/ { print $2; exit }' "$work/node-sums")
  [ -n "$file" ] || fail "nodejs.org has no Node $NODE_MAJOR for $1"
  say "Installing ${file%.tar.gz} into $home/runtime" >&2
  fetch "$base/$file" "$work/$file" || fail "could not download $file"
  verify "$work/$file" "$file" "$work/node-sums"
  rm -rf "$home/runtime.new"
  mkdir -p "$home/runtime.new"
  tar -xzf "$work/$file" -C "$home/runtime.new" --strip-components 1
  rm -rf "$home/runtime"
  mv "$home/runtime.new" "$home/runtime"
  echo "$home/runtime/bin/node"
}

# Adds $OME_HOME/bin to PATH in the profile of the user's shell.
modify_path() {
  bin="$home/bin"
  case ":$PATH:" in *":$bin:"*) return 0 ;; esac
  case ${SHELL:-} in
    */zsh) profile="${ZDOTDIR:-$HOME}/.zshrc" line="export PATH=\"$bin:\$PATH\"" ;;
    */bash) profile="$HOME/.bashrc" line="export PATH=\"$bin:\$PATH\"" ;;
    */fish) profile="$HOME/.config/fish/conf.d/ome.fish" line="fish_add_path \"$bin\"" ;;
    *) profile="$HOME/.profile" line="export PATH=\"$bin:\$PATH\"" ;;
  esac
  if [ -f "$profile" ] && grep -qF "$bin" "$profile"; then
    return 0
  fi
  mkdir -p "$(dirname "$profile")"
  printf '\n# Ome\n%s\n' "$line" >>"$profile"
  say "Added $bin to PATH in $profile. Open a new terminal, or run: $line"
}

main() {
  version="${OME_VERSION:-}"
  modify=1
  while [ $# -gt 0 ]; do
    case $1 in
      --version)
        [ $# -ge 2 ] || fail "--version needs a value"
        version=$2
        shift 2
        ;;
      --version=*)
        version=${1#--version=}
        shift
        ;;
      --no-modify-path)
        modify=0
        shift
        ;;
      -h | --help)
        usage
        return 0
        ;;
      *) fail "unknown option $1 (see --help)" ;;
    esac
  done

  home="${OME_HOME:-$HOME/.ome}"
  platform=$(detect_platform)
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT INT TERM

  [ -n "$version" ] || version=$(latest_version)
  version=${version#v}
  base="${OME_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download/v$version}"
  archive="ome-$version-$platform.tar.gz"

  say "Installing Ome $version for $platform into $home"
  fetch "$base/SHA256SUMS" "$work/SHA256SUMS" || fail "could not download SHA256SUMS for $version"
  fetch "$base/$archive" "$work/$archive" || fail "could not download $archive"
  verify "$work/$archive" "$archive" "$work/SHA256SUMS"
  if [ -z "${OME_DOWNLOAD_BASE:-}" ] && has gh && gh auth status >/dev/null 2>&1; then
    gh attestation verify "$work/$archive" --repo "$REPO" >/dev/null ||
      fail "$archive has no valid GitHub attestation from $REPO"
    say "Checked the build attestation."
  fi

  node=$(system_node)
  [ -n "$node" ] || node=$(install_node "$platform")

  mkdir -p "$home/versions" "$home/bin"
  target="$home/versions/$version"
  rm -rf "$target.new"
  mkdir -p "$target.new"
  tar -xzf "$work/$archive" -C "$target.new" --strip-components 1
  # Services start the daemon without PATH: it runs with the Node found now.
  sed "s|^node_default=\"\"\$|node_default=\"$node\"|" "$target.new/bin/omed" >"$work/omed"
  cat "$work/omed" >"$target.new/bin/omed"
  rm -rf "$target"
  mv "$target.new" "$target"

  previous=$(readlink "$home/current" 2>/dev/null || true)
  ln -sfn "versions/$version" "$home/current.new"
  mv -f "$home/current.new" "$home/current" 2>/dev/null || {
    rm -f "$home/current"
    mv "$home/current.new" "$home/current"
  }
  for program in ome omed ome-keeper; do
    ln -sfn "../current/bin/$program" "$home/bin/$program"
  done
  # Keep this version and the one before it (a running daemon may still use it).
  for old in "$home"/versions/*; do
    [ -d "$old" ] || continue
    case $old in
      "$target" | "$home/$previous") ;;
      *) rm -rf "$old" ;;
    esac
  done

  [ "$modify" = 1 ] && modify_path
  say ""
  say "Installed $("$home/bin/ome" --version) (daemon on Node $("$node" --version))."
  say "Next: ome init, then ome doctor."
}

main "$@"
