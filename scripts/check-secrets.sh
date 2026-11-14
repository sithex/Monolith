#!/usr/bin/env bash
# Scan index blobs, never working-tree contents. Requires only Git, Bash and awk.
set -euo pipefail
export LC_ALL=C
case ${1-} in
  '') mode=staged ;;
  --all) mode=all ;;
  *) printf 'Usage: %s [--all]\n' "$0" >&2; exit 2 ;;
esac
[[ $# -le 1 ]] || exit 2
cd "$(git rev-parse --show-toplevel)"
umask 077
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
trap 'exit 2' HUP INT TERM

# An unstaged allowlist edit must not silently exempt staged secrets.
if git cat-file -e :scripts/secrets-allowlist.txt 2>/dev/null; then
  git show :scripts/secrets-allowlist.txt > "$scratch/allowlist"
else
  : > "$scratch/allowlist"
fi
if [[ $mode == all ]]; then
  git ls-files -z > "$scratch/paths"
else
  git diff --cached --name-only -z --diff-filter=ACMRT > "$scratch/paths"
fi
cat > "$scratch/scan.awk" <<'AWK'
function allowed(kind, value,    i, record) {
    record = ENVIRON["SCAN_PATH"] ":" kind ":" value
    for (i = 1; i <= exceptions; i++)
        if (record ~ exception[i]) return 1
    return 0
}
function report(kind, value, line) {
    if (!allowed(kind, value)) print line ": " kind " [REDACTED]"
}
function scan(kind, pattern,    rest, value) {
    rest = $0
    while (match(rest, pattern)) {
        value = substr(rest, RSTART, RLENGTH)
        report(kind, value, FNR)
        rest = substr(rest, RSTART + RLENGTH)
    }
}
BEGIN {
    while ((getline rule < ARGV[1]) > 0) {
        if (rule ~ /^[[:space:]]*(#|$)/) continue
        # Force invalid EREs to fail even when there are no matching secrets.
        valid = ("" ~ rule)
        exception[++exceptions] = rule
    }
    close(ARGV[1])
    ARGV[1] = ""
    path = ENVIRON["SCAN_PATH"]
    if (path ~ /(^|\/)\.env($|\.)/) report("env-file", path, 0)
}
{
    scan("private-key", "-----BEGIN ([A-Z0-9]+ )*PRIVATE KEY-----")
    scan("github-token", "ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}")
    scan("api-token", "sk-[A-Za-z0-9_-]{20,}")
    scan("slack-token", "xox[baprsce]-[A-Za-z0-9-]{10,}|xapp-[A-Za-z0-9-]{10,}")
    scan("aws-access-key", "AKIA[A-Z0-9]{16}")
    # All 100.x addresses are flagged, including the CGNAT/Tailscale range.
    rest = $0
    while (match(rest, /(^|[^0-9.])100\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}([^0-9.]|$)/)) {
        value = substr(rest, RSTART, RLENGTH)
        sub(/^[^0-9]/, "", value)
        sub(/[^0-9]$/, "", value)
        split(value, octets, ".")
        if (octets[2] <= 255 && octets[3] <= 255 && octets[4] <= 255)
            report("tailscale-ip", value, FNR)
        rest = substr(rest, RSTART + RLENGTH - (substr(rest, RSTART + RLENGTH - 1, 1) ~ /[^0-9.]/))
    }
    # Private machine names (case-insensitive) must not reach the public repo.
    lower = tolower($0)
    rest = lower
    while (match(rest, /nick[-]pc|nicks[-]laptop|one[p]lus/)) {
        report("host-name", substr(rest, RSTART, RLENGTH), FNR)
        rest = substr(rest, RSTART + RLENGTH)
    }
    # Include the current user too: public-repo privacy must not depend on who commits.
    scan("home-path", "/(home|Users)/[A-Za-z0-9_.-]+(/[A-Za-z0-9_.@+/-]*)?")
}
AWK
failed=0
while IFS= read -r -d '' path; do
  # git show reads symlink blobs too, without following links outside the repo.
  if [[ $(git cat-file -t ":$path") == commit ]]; then continue; fi
  git show ":$path" > "$scratch/blob"
  if ! SCAN_PATH="$path" awk -f "$scratch/scan.awk" "$scratch/allowlist" "$scratch/blob" > "$scratch/report" 2> "$scratch/error"; then
    printf 'Secrets scan could not read its rules/content (details suppressed).\n' >&2
    exit 2
  fi
  if [[ -s $scratch/report ]]; then
    printf 'Secrets scan: %q\n' "$path" >&2
    cat "$scratch/report" >&2
    failed=1
  fi
done < "$scratch/paths"
if (( failed )); then
  printf 'Commit blocked. Remove sensitive content or add a narrowly scoped, reviewed regex to scripts/secrets-allowlist.txt and stage it.\n' >&2
  exit 1
fi
printf 'Secrets scan passed (%s index content).\n' "$mode"
