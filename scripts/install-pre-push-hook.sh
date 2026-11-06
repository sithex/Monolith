#!/usr/bin/env bash
# Opt-in git pre-push hook that runs scripts/precheck.sh. Nothing installs itself: run this
# yourself if you want it.
#
#   scripts/install-pre-push-hook.sh            install (refuses to overwrite a different hook)
#   scripts/install-pre-push-hook.sh --remove   remove the hook if it is ours
#
# The hook lands in the repo's effective hooks directory (`core.hooksPath`, normally
# `.githooks`). Bypass it once with `git push --no-verify`.
set -euo pipefail
cd -- "$(git rev-parse --show-toplevel)"
hook="$(git rev-parse --git-path hooks)/pre-push"
marker='# monolith-precheck-hook'
case "${1-}" in
  --remove)
    if [[ -f "$hook" ]] && grep -q "$marker" "$hook"; then
      rm -- "$hook"
      echo "removed $hook"
    else
      echo "no Monolith pre-push hook at $hook"
    fi
    exit 0
    ;;
  '') ;;
  *) echo "Usage: $0 [--remove]" >&2; exit 2 ;;
esac
if [[ -e "$hook" ]] && ! grep -q "$marker" "$hook"; then
  echo "refusing to overwrite $hook (not created by this script)" >&2
  exit 1
fi
mkdir -p -- "$(dirname -- "$hook")"
cat >"$hook" <<EOF
#!/bin/sh
$marker
exec bash "\$(git rev-parse --show-toplevel)/scripts/precheck.sh"
EOF
chmod 755 "$hook"
echo "installed $hook"
