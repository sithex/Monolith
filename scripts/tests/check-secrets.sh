#!/usr/bin/env bash
# Isolated temporary repositories: no hooks/config/index changes in the checkout.
set -euo pipefail
scanner=$(cd "$(dirname "$0")/.." && pwd)/check-secrets.sh
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
git init -q "$scratch/repo"
cd "$scratch/repo"
mkdir scripts
cp "$scanner" scripts/check-secrets.sh
touch scripts/secrets-allowlist.txt
git add scripts
git -c user.name=Test -c user.email=test@example.invalid -c core.hooksPath=/dev/null commit -qm baseline
checks=0
expect() {
  local want=$1 result=0
  shift
  bash scripts/check-secrets.sh "$@" > "$scratch/report" 2>&1 || result=$?
  if [[ $result != "$want" ]]; then
    printf 'FAIL: expected exit %s, got %s (report suppressed)\n' "$want" "$result" >&2
    exit 1
  fi
  checks=$((checks + 1))
}
expect 0
tokens=(
  "-----BEGIN OPENSSH PRIVATE"' KEY-----'
  'ghp_'"$(printf '%040d' 0)"
  'github_pat_'"$(printf '%080d' 0)"
  'sk-'"$(printf '%048d' 0)"
  'sk-ant-api03-'"$(printf '%048d' 0)"
  'xoxb-'"$(printf '%024d' 0)"
  'xapp-'"$(printf '%024d' 0)"
  'AKIA'"$(printf '%016d' 0)"
  '100.'"101.102.103"
  '/home/'"someone-else/private.txt"
  '/Users/'"someone-else/private.txt"
  'nick'"-pc.tailnet.ts.net"
  'Nicks'"-Laptop"
)
for token in "${tokens[@]}"; do
  printf '%s\n' "$token" > 'fixture with spaces.txt'
  git add 'fixture with spaces.txt'
  expect 1
  if grep -Fq -- "$token" "$scratch/report"; then
    printf 'FAIL: scan printed a secret value\n' >&2; exit 1
  fi
done
# Staged secret, clean working tree: still blocked.
printf 'clean\n' > 'fixture with spaces.txt'
expect 1
# Staged clean, dirty working tree: passes.
git add 'fixture with spaces.txt'
printf '%s\n' "${tokens[1]}" > 'fixture with spaces.txt'
expect 0
# NUL-delimited filename handling, renames and deletions.
odd=$'line\nbreak.txt'
printf '%s\n' "${tokens[1]}" > "$odd"
git add "$odd"
expect 1
git rm -q --cached "$odd"
expect 0
touch .env .env.production
git add .env .env.production
expect 1
git rm -q --cached .env .env.production
expect 0
# Per-match exception cannot hide a second credential on the same line.
printf '%s %s\n' "${tokens[9]}" "${tokens[1]}" > fixture
git add fixture
printf '^fixture:home-path:/[h]ome/%s$\n' 'someone-else/private[.]txt' > scripts/secrets-allowlist.txt
git add scripts/secrets-allowlist.txt
expect 1
printf '%s\n' "${tokens[9]}" > fixture
git add fixture
expect 0
# Working-tree exceptions do not affect the staged allowlist.
: > scripts/secrets-allowlist.txt
git add scripts/secrets-allowlist.txt
printf '.*\n' > scripts/secrets-allowlist.txt
expect 1
printf '[\n' > scripts/secrets-allowlist.txt
git add scripts/secrets-allowlist.txt
expect 2
: > scripts/secrets-allowlist.txt
git add scripts/secrets-allowlist.txt
git -c user.name=Test -c user.email=test@example.invalid -c core.hooksPath=/dev/null commit -qm fixtures
expect 0
expect 1 --all
git mv fixture renamed
expect 1
git rm -qf renamed
expect 0
# Scan a staged symlink target as text, without following the link.
ln -s "${tokens[9]}" link
git add link
expect 1
printf 'Passed %s secrets-hook checks.\n' "$checks"
