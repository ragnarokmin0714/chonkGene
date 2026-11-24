#!/bin/sh
# Her whole figure for the app: made in Blender (figure_blend.py), each mesh
# simplified to what a pet a couple of hundred pixels tall needs
# (tools/meshbake) and gzipped, beside the model that names them, into
# figure/ (embedded by chonkgene-render's `skin::human`).
#
#   sh assets/species/fox/build_figure.sh
#
# Needs Blender (4.0 is what it was made with) and cargo. The metaballs come
# out a little differently each run, so run it only when the figure changes.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
blender -b --factory-startup -P "$here/figure_blend.py" -- "$work" | grep -E "vertices|tails:|Error|Traceback" || true
test -f "$work/human_figure.ron" || { echo "Blender wrote no model" >&2; exit 1; }
cargo build -q --release --manifest-path "$root/tools/meshbake/Cargo.toml"
mkdir -p "$here/figure"
for f in "$work"/*.mesh; do
    name=$(basename "$f" .mesh)
    "$root/tools/meshbake/target/release/meshbake" "$f" "$work/$name.small"
    gzip -9n < "$work/$name.small" > "$here/figure/$name.mesh.gz"
done
cp "$work/human_figure.ron" "$here/figure/figure.ron"
ls -l "$here/figure"
