#!/bin/sh
set -eu

repository_root=$(git rev-parse --show-toplevel)
git -C "$repository_root" config core.hooksPath .githooks

for hook in pre-commit pre-push; do
  hook_path="$repository_root/.githooks/$hook"
  if [ -e "$hook_path" ] && [ ! -x "$hook_path" ]; then
    echo "Git hook is not executable: .githooks/$hook" >&2
    exit 1
  fi
done

echo "Git hooks enabled from .githooks"
