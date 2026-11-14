#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "Refusing to apply patches to a dirty working tree." >&2
    exit 1
fi

base_file="$repo_root/patches/base-revision"
if [[ ! -f "$base_file" ]]; then
    echo "Missing patches/base-revision." >&2
    exit 1
fi

base_revision="$(<"$base_file")"
if [[ "$(git rev-parse HEAD)" != "$base_revision" ]]; then
    echo "Patch stack expects base revision $base_revision." >&2
    echo "Current checkout is $(git rev-parse HEAD)." >&2
    exit 1
fi

shopt -s nullglob
patches=("$repo_root"/patches/[0-9][0-9][0-9][0-9]-*.patch)
if ((${#patches[@]} == 0)); then
    echo "No patch files found in patches/." >&2
    exit 1
fi

for patch_file in "${patches[@]}"; do
    echo "Applying ${patch_file##*/}"
    git apply --check "$patch_file"
    git apply "$patch_file"
done
