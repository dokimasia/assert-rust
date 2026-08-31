#!/usr/bin/env sh
# Hold the vendored copy of the definition to two questions.
#
# Is it intact? Every file is compared against the manifest vendored
# beside it. A hand-edited or half-copied file fails, offline.
#
# Does it match upstream? The manifest at the pinned ref is fetched and
# the digests compared. This reports by default, because an
# implementation is allowed to lag a change while it catches up.
#
# Pass --strict to make a difference fail. CI uses that on a change that
# touches the vendored copy: falling behind is fine, and committing a
# copy that matches nothing anyone else has is not.
set -eu

STRICT=0
for arg in "$@"; do
    [ "$arg" = "--strict" ] && STRICT=1
done
DEST=${1:?usage: spec-check.sh <vendored directory> [--strict]}
REF=${SPEC_REF:-main}
RAW="https://raw.githubusercontent.com/dokimasia/assert-spec/$REF"

python3 - "$DEST" "$(dirname "$0")" <<'PY_INNER'
import hashlib, json, pathlib, sys

dest = pathlib.Path(sys.argv[1])
tools = pathlib.Path(sys.argv[2])
manifest = json.loads((dest / "manifest.json").read_text())

def local(name):
    if name.startswith("corpus/"):
        return dest / name
    if name.startswith("overlays/"):
        return None
    if name.startswith("tools/"):
        return tools / pathlib.Path(name).name
    return dest / pathlib.Path(name).name

checked, wrong, missing = 0, [], []
for name, want in sorted(manifest["files"].items()):
    path = local(name)
    if path is None:
        continue
    if not path.exists():
        missing.append(name)
        continue
    got = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    checked += 1
    if got != want:
        wrong.append(name)

if missing:
    print("spec: vendored copy is missing " + ", ".join(missing))
if wrong:
    print("spec: these do not match the manifest beside them: " + ", ".join(wrong))
if missing or wrong:
    raise SystemExit(1)

print(f"spec: {checked} files intact at {manifest['version']} {manifest['digest'][:19]}")
PY_INNER

mine=$(python3 -c "import json;print(json.load(open('$DEST/manifest.json'))['digest'])")
theirs=$(curl -fsSL --max-time 20 "$RAW/spec/manifest.json" 2>/dev/null \
    | python3 -c "import json,sys;print(json.load(sys.stdin)['digest'])" 2>/dev/null) || theirs=""

if [ -z "$theirs" ]; then
    echo "spec: could not reach $REF, so it was not compared"
    exit 0
fi

if [ "$mine" = "$theirs" ]; then
    echo "spec: matches $REF"
    exit 0
fi

# Behind, ahead, or taken from a checkout nobody pushed. One fetch
# cannot tell those apart, so it says what it knows.
echo "spec: differs from $REF"
echo "  vendored $mine"
echo "  upstream $theirs"
echo "  run: ./tools/spec-sync.sh $DEST <language>"

if [ "$STRICT" = "1" ]; then
    echo "spec: this change touches the vendored copy, so it has to match"
    exit 1
fi
