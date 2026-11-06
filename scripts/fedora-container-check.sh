#!/usr/bin/env bash
# Build and test this checkout on a fresh Fedora in Docker.
#
#   scripts/fedora-container-check.sh [--image fedora:latest] [--name NAME] [--keep-going]
#
# Exports `git archive HEAD` of the repo this script lives in (uncommitted changes are NOT
# included; the repo is never mounted), then in a new container, as root:
#   scripts/install-deps.sh --with-test-deps --yes, a check that every --with-optional package
#   exists, scripts/tests/install-deps.sh, the Qt 6 tool checks, npm ci, ./build.sh,
#   cargo test --locked --lib --test units --test services, npm run -s typecheck, npm run -s test:ui.
# It stops at the first failing step unless --keep-going. The container is removed on exit.
#
# Caches (cargo registry, npm, dnf) and logs live in $FEDORA_CHECK_CACHE
# (default ~/.hermes/cache/scratch/monolith-fedora). Docker limits: --cpus=8 --cpu-shares=256.
# Known, not distro-specific: on Node < 26 one palette test fails (Node EventTarget bug);
# it is reported as KNOWN and does not fail the run.
# The step snippets are single-quoted on purpose: they expand inside the container.
# shellcheck disable=SC2016
set -euo pipefail

image=fedora:latest
name="monolith-fedora-check-$$"
keep_going=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --image) shift; image=${1:?--image needs a value} ;;
    --name) shift; name=${1:?--name needs a value} ;;
    --keep-going) keep_going=1 ;;
    -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
    *) printf 'fedora-check: unknown option %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done

repo=$(git -C "$(dirname -- "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
head=$(git -C "$repo" rev-parse --short=12 HEAD)
cache=${FEDORA_CHECK_CACHE:-$HOME/.hermes/cache/scratch/monolith-fedora}
logs="$cache/logs/check-$head-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$cache/cargo-registry" "$cache/npm-cache" "$cache/dnf-cache" "$logs"
input=$(mktemp -d "${TMPDIR:-/tmp}/fedora-check.XXXXXX")

cleanup() {
  # Docker runs as root: hand the shared caches back to the caller before removing the container.
  docker exec "$name" chown -R "$(id -u):$(id -g)" /root/.cargo/registry /root/.npm /var/cache/libdnf5 \
    >/dev/null 2>&1 || true
  docker rm -f "$name" >/dev/null 2>&1 || true
  rm -rf -- "$input"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

if [[ -n "$(git -C "$repo" status --porcelain --untracked-files=no)" ]]; then
  echo "fedora-check: note: uncommitted changes in $repo are not part of HEAD and are not tested"
fi
git -C "$repo" archive --format=tar HEAD >"$input/src.tar"

echo "fedora-check: $repo @ $head on $image, logs in $logs"
docker run -d --name "$name" --cpus=8 --cpu-shares=256 -w /work \
  -v "$input:/in:ro" \
  -v "$cache/cargo-registry:/root/.cargo/registry" \
  -v "$cache/npm-cache:/root/.npm" \
  -v "$cache/dnf-cache:/var/cache/libdnf5" \
  -e TMPDIR=/tmp/t -e CARGO_BUILD_JOBS=8 -e CI=1 \
  "$image" sleep infinity >"$logs/00-container.log"

failed=0
results=()
step() { # key, shell snippet run in /work inside the container
  local key="$1" snippet="$2" start rc=0 verdict
  if [[ "$failed" == 1 && "$keep_going" == 0 ]]; then
    results+=("$(printf '%-19s SKIP' "$key")")
    return 0
  fi
  start=$SECONDS
  docker exec -w /work "$name" bash -euo pipefail -c "$snippet" >"$logs/$key.log" 2>&1 || rc=$?
  verdict=PASS
  if [[ "$rc" != 0 ]]; then
    verdict=FAIL
    if [[ "$key" == test-ui ]] && known_ui_failure "$logs/$key.log"; then verdict=KNOWN; fi
  fi
  [[ "$verdict" == FAIL ]] && failed=1
  results+=("$(printf '%-19s %-5s %4ss  %s' "$key" "$verdict" "$((SECONDS - start))" "$(summary "$key")")")
  printf 'fedora-check: %s\n' "${results[-1]}"
}
# Exactly one failing test, the palette one, on Node < 26.
known_ui_failure() {
  local log="$1" fails major
  fails=$(grep -cE '^not ok ' "$log" || true)
  major=$(docker exec "$name" node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
  [[ "$fails" == 1 && "$major" -lt 26 ]] && grep -qE '^not ok .*palette captures Ctrl\+K' "$log"
}
summary() { # the result lines worth pasting
  local log="$logs/$1.log"
  case "$1" in
    cargo-test) grep -E '^test result:' "$log" | sed -E 's/; finished in .*//' | paste -sd'|' - ;;
    test-ui) grep -E '^# (tests|pass|fail) ' "$log" | paste -sd' ' - ;;
    install-deps-checks | optional-names | qt-tools) tail -n 1 "$log" ;;
    *) : ;;
  esac
}

step extract 'mkdir -p /tmp/t && tar -xf /in/src.tar -C /work && grep -E "^(PRETTY_NAME|VERSION_ID)=" /etc/os-release'
step install-deps 'grep -q "^keepcache=" /etc/dnf/dnf.conf || echo keepcache=True >>/etc/dnf/dnf.conf
  bash scripts/install-deps.sh --with-test-deps --yes
  rustc --version; cargo --version; node --version; npm --version'
step optional-names 'read -r -a cmd <<<"$(bash scripts/install-deps.sh --print --with-optional --with-test-deps)"
  [[ "${cmd[*]:0:2}" == "dnf install" ]] || { echo "unexpected command: ${cmd[*]}"; exit 1; }
  wanted=("${cmd[@]:2}")
  found=$(dnf -q repoquery --qf "%{name}\n" "${wanted[@]}" | sort -u)
  missing=()
  for p in "${wanted[@]}"; do
    # A name dnf install accepts: a package, or a capability one provides (npm -> nodejs-npm).
    grep -qx -- "$p" <<<"$found" || [[ -n "$(dnf -q repoquery --whatprovides "$p")" ]] || missing+=("$p")
  done
  ((${#missing[@]} == 0)) || { echo "not in the Fedora repos: ${missing[*]}"; exit 1; }
  echo "all ${#wanted[@]} Fedora package names resolve"'
step install-deps-checks 'bash scripts/tests/install-deps.sh'
step qt-tools 'test -x /usr/bin/qdbus-qt6 && test ! -e /usr/bin/qdbus6 && ls -l /usr/bin/qdbus-qt6 /usr/lib64/qt6/bin/qdbus
  python3 -m unittest discover -s island/qmlrenders -p "test_*.py"
  source <(sed -n "/^qt6_tool() {/,/^}/p" scripts/precheck.sh)
  [[ "$(qt6_tool qdbus)" == /usr/lib64/qt6/bin/qdbus ]]
  echo "Qt 6 qdbus: $(qt6_tool qdbus) (no /usr/bin/qdbus6); qt6_tools ok"'
step npm-ci 'cd island-tauri && npm ci --no-audit --no-fund'
step build 'cd island-tauri && ./build.sh && ls -l src-tauri/target/release/hermes-island'
step cargo-test 'cd island-tauri/src-tauri && cargo test --locked --no-fail-fast --lib --test units --test services'
step typecheck 'cd island-tauri && npm run -s typecheck && echo typecheck ok'
step test-ui 'cd island-tauri && node --version && npm run -s test:ui -- --test-reporter=tap'

printf '\nfedora-check: %s @ %s on %s\n' "$repo" "$head" "$image"
printf '%s\n' "${results[@]}"
if [[ "$failed" == 1 ]]; then
  printf 'fedora-check: FAIL (logs in %s)\n' "$logs"
  exit 1
fi
printf 'fedora-check: PASS (logs in %s)\n' "$logs"
