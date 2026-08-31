#!/usr/bin/env sh
# Refresh the vendored copy of the definition.
#
# Fetches a pinned ref, so the same command answers the same way on a
# laptop and on a runner. That is the whole point: a sync whose result
# depends on what happens to be in a sibling directory is a sync nobody
# can reason about, and a copy taken from an uncommitted checkout agrees
# with the manifest beside it while matching nothing anyone else has.
#
# Set SPEC_LOCAL to take a sibling checkout instead, which is how an
# unpushed change gets tried. It says so loudly, because the copy it
# leaves behind is one no other machine can reproduce.
set -eu

DEST=${1:?usage: spec-sync.sh <destination directory> [overlay language]}
LANG=${2:-}
REF=${SPEC_REF:-main}
RAW="https://raw.githubusercontent.com/dokimasia/assert-spec/$REF"

FILES="VERSION spec/assertions.json spec/naming.json spec/manifest.json"

fetch() {
    # $1 repository-relative path, $2 where it lands
    if [ -n "${SPEC_LOCAL:-}" ]; then
        cp "$SPEC_LOCAL/$1" "$2"
    else
        curl -fsSL "$RAW/$1" -o "$2"
    fi
}

if [ -n "${SPEC_LOCAL:-}" ]; then
    echo "spec: taking $SPEC_LOCAL, not $REF"
    echo "spec: the copy this leaves is reproducible nowhere else; do not commit it"
fi

mkdir -p "$DEST/corpus"

for f in $FILES; do
    fetch "$f" "$DEST/$(basename "$f")"
done

if [ -n "$LANG" ]; then
    fetch "overlays/$LANG.json" "$DEST/overlay.json"
fi

# The manifest names the corpus, so it decides what to copy rather than
# a glob that would quietly keep a file the definition dropped.
names=$(python3 -c "
import json
m = json.load(open('$DEST/manifest.json'))
print(' '.join(n[len('corpus/'):-len('.json')] for n in m['files'] if n.startswith('corpus/')))")

rm -f "$DEST"/corpus/*.json
for name in $names; do
    fetch "corpus/$name.json" "$DEST/corpus/$name.json"
done

# The tooling is vendored the same way the definition is, or five
# copies of it drift exactly as the definition did. Renamed into place
# so the copy running now is never written through.
TOOLS=$(dirname "$0")
for script in spec-sync.sh spec-check.sh; do
    fetch "tools/$script" "$TOOLS/$script.new"
    chmod +x "$TOOLS/$script.new"
    mv "$TOOLS/$script.new" "$TOOLS/$script"
done

[ -n "${SPEC_LOCAL:-}" ] || echo "spec: fetched $REF"
exec "$(dirname "$0")/spec-check.sh" "$DEST"
