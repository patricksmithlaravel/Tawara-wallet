#!/usr/bin/env bash
# Writes THIRD-PARTY-NOTICES.txt from Cargo.lock, or checks the committed copy
# against what the generator writes now (docs/DECISIONS.md D35).
#
#   bash ci/notices.sh            regenerate THIRD-PARTY-NOTICES.txt
#   bash ci/notices.sh --check    fail, with a diff, if the committed file
#                                 differs; CI runs this
#
# The generator is cargo-about 0.9.2, installed with
#   cargo install --locked --features cli cargo-about --version 0.9.2
# Without `--features cli` that command installs nothing and still exits 0. So
# this script also refuses to run with any other version. Its settings are in
# about.toml.
#
# Three things are done to the generated text (D35 item 2), and the words of
# the licences do not change:
#
# - Carriage returns are removed. Some licence files are published with CRLF
#   line endings, but every text file in this repository and in Tawara-mobile
#   is kept with LF (.gitattributes). A CRLF copy would be rewritten on commit,
#   and then could never match the generator again.
# - The template line of the standard MIT text, "Copyright (c) <year>
#   <copyright holders>", is removed, with the blank line after it. cargo-about
#   prints the standard text for a crate that publishes no text of its own, and
#   the template line would read as a copyright notice that names nobody. The
#   crates concerned are named in about.toml. Any other template marker left in
#   the text stops the script: it means a crate fell back to a standard text
#   that about.toml does not account for, and needs a clarification.
# - The Mochimo agreement's text is appended from LICENSE.md, because
#   cargo-about 0.9.2 does not print it (see about.toml).
set -euo pipefail

about_version="0.9.2"

mode="write"
case "${1:-}" in
  "") ;;
  --check) mode="check" ;;
  *)
    echo "usage: bash ci/notices.sh [--check]" >&2
    exit 2
    ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

found=$(cargo about --version 2>/dev/null || true)
if [ "$found" != "cargo-about $about_version" ]; then
  echo "ci/notices.sh needs cargo-about $about_version, and found '${found:-no cargo-about}'." >&2
  echo "Install it with: cargo install --locked --features cli cargo-about --version $about_version" >&2
  exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# --workspace: this repository's crates are noticed with their dependencies.
# --all-features: as deny.toml's `[graph]` table does; it changes nothing while
#   the workspace's own crates declare no features.
# --fail: a crate whose licence cannot be worked out at all stops the run. A
#   clarification file that cannot be read only warns, so --check, which compares
#   the output byte for byte, is what catches a change that such a file causes.
# --locked: the run uses Cargo.lock exactly, and cannot change it.
# -L warn: the generator's warnings are printed. Seven are expected: three
#   about the Mochimo agreement, and four for the objc2 crates whose newer
#   versions have no LICENSE.txt to fetch; see about.toml.
# The `git` clarifications in about.toml fetch files from
# raw.githubusercontent.com, so the run needs the network.
cargo about -L warn generate --workspace --all-features --locked --fail \
  -c about.toml -o "$work/generated.txt" THIRD-PARTY-NOTICES.hbs

template_line='Copyright (c) <year> <copyright holders>'
tr -d '\r' < "$work/generated.txt" |
  awk -v line="$template_line" '
    skip && $0 == "" { skip = 0; next }
    { skip = 0 }
    $0 == line { skip = 1; next }
    { print }
  ' > "$work/generated-lf.txt"

if grep -n -i -E '<(year|copyright holders?|owner|name of author)>' "$work/generated-lf.txt" >&2; then
  cat >&2 <<'EOF'
The generated notices still hold a licence template's placeholder (above). A
crate has fallen back to a standard licence text that names no holder. Give it
its own text with a clarification in about.toml (docs/DECISIONS.md D35,
item 6 (a)).
EOF
  exit 1
fi

cat "$work/generated-lf.txt" LICENSE.md > "$work/THIRD-PARTY-NOTICES.txt"

if [ "$mode" = write ]; then
  cp "$work/THIRD-PARTY-NOTICES.txt" THIRD-PARTY-NOTICES.txt
  echo "wrote THIRD-PARTY-NOTICES.txt"
  exit 0
fi

if ! cmp -s THIRD-PARTY-NOTICES.txt "$work/THIRD-PARTY-NOTICES.txt"; then
  diff -u THIRD-PARTY-NOTICES.txt "$work/THIRD-PARTY-NOTICES.txt" | head -n 60 >&2 || true
  cat >&2 <<'EOF'
THIRD-PARTY-NOTICES.txt is not what Cargo.lock and about.toml give now.
To regenerate it, install cargo-about 0.9.2 as above, then run
  bash ci/notices.sh
and commit the file it writes (docs/DECISIONS.md D35).
EOF
  exit 1
fi
echo "THIRD-PARTY-NOTICES.txt is current"
