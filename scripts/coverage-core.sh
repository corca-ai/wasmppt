#!/usr/bin/env sh
set -eu

mkdir -p target/coverage
cargo llvm-cov clean --workspace
cargo llvm-cov --locked --all-features --no-report \
  -p wasmppt-opc -p wasmppt-xml -p wasmppt-pml -p wasmppt-template \
  -p wasmppt-layout -p wasmppt-metafile -p wasmppt-display -p wasmppt-shaper
cargo llvm-cov report --summary-only --json --output-path target/coverage/summary.json
cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
node scripts/check-coverage.mjs
