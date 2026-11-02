#!/usr/bin/env bash
# Builds the WebKitGTK vblank-match shim used by run.sh (see vblank_fix.c for why).
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
gcc -O2 -Wall -Wextra -Werror -shared -fPIC -o libvblank_fix.so vblank_fix.c -ldl
echo built "$PWD/libvblank_fix.so"
