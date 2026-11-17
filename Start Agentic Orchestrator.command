#!/bin/sh
# Launch from Finder or any working directory. Build again after pulling commits.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXE="$ROOT/target/release/agentic-orchestrator"
STAMP="$ROOT/target/release/agentic-orchestrator.macos.built"

failed() {
  printf '\n%s\n' "The build or launch did not finish. Fix the problem above and run this file again."
  if [ -t 0 ]; then
    printf 'Press Return to close… '
    read -r ignored || true
  fi
  exit 1
}
trap failed HUP INT TERM

HEAD=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null) || HEAD=unknown
BUILT=
if [ -f "$STAMP" ]; then BUILT=$(cat "$STAMP"); fi

if [ ! -x "$EXE" ] || [ "$HEAD" = unknown ] || [ "$BUILT" != "$HEAD" ]; then
  printf '%s\n' "Building Agentic Orchestrator. The first build takes about five minutes."
  for tool in git pnpm cargo claude; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      printf '%s was not found on PATH. See Requirements in README.md.\n' "$tool"
      failed
    fi
  done
  CI=true pnpm -C "$ROOT/app" install --frozen-lockfile || failed
  pnpm -C "$ROOT/app" tauri build --no-bundle || failed
  printf '%s\n' "$HEAD" > "$STAMP" || failed
fi

"$EXE" || failed
