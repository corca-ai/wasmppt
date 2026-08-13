#!/usr/bin/env sh
set -eu

fuzz_root=crates/wasmppt-opc/fuzz
artifact_root=target/fuzz-artifacts
duration=${WASMPPT_FUZZ_SECONDS:-30}

case "$duration" in
  ''|*[!0-9]*) echo "WASMPPT_FUZZ_SECONDS must be a positive integer" >&2; exit 2 ;;
esac
if [ "$duration" -lt 1 ]; then
  echo "WASMPPT_FUZZ_SECONDS must be a positive integer" >&2
  exit 2
fi

mkdir -p "$artifact_root"
{
  echo "cargo-fuzz=$(cargo fuzz --version)"
  echo "rustc=$(rustc --version)"
  echo "seconds-per-target=$duration"
  echo "targets=open_package package_graph slide_geometry template_bindings xml_tokens"
} > "$artifact_root/metadata.txt"

for target in open_package package_graph slide_geometry template_bindings xml_tokens; do
  mkdir -p "$artifact_root/$target"
  cargo fuzz run --fuzz-dir "$fuzz_root" "$target" -- \
    "-max_total_time=$duration" "-artifact_prefix=$artifact_root/$target/"
done

{
  find "$fuzz_root/corpus" -type f -exec sha256sum {} + 2>/dev/null || true
} >> "$artifact_root/metadata.txt"
