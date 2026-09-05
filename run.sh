#!/usr/bin/env bash
#
# Build and run the app, moving to the nearest free port if the preferred one is
# taken (searched in both directions, never below 1024).
#
# Usage:
#   ./run.sh                 # build if needed, serve on 3000 or the nearest free port
#   ./run.sh 8080            # prefer port 8080
#   PORT=8080 ./run.sh       # same via env
#   ./run.sh --release       # release build
#   ./run.sh --no-build      # skip building, just serve what is in target/
#   ./run.sh --kill          # free the preferred port instead of moving to another
#
# When the preferred port is busy the nearest free one is used, searching both
# upwards and downwards (never below 1024), and ties prefer the higher port.
#
set -euo pipefail

cd "$(dirname "$0")"

PREFERRED_PORT="${PORT:-3000}"
HOST="${HOST:-127.0.0.1}"
PROFILE="debug"
DO_BUILD=1
KILL_OCCUPANT=0
MAX_PORT_TRIES="${MAX_PORT_TRIES:-50}"

log()  { printf '\033[0;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[0;33m warn\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[0;31merror\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --release)  PROFILE="release" ;;
    --no-build) DO_BUILD=0 ;;
    --kill)     KILL_OCCUPANT=1 ;;
    --host)     shift; HOST="${1:?--host needs a value}" ;;
    -h|--help)  sed -n '3,16p' "$0" | sed 's/^#\{1\} \{0,1\}//'; exit 0 ;;
    [0-9]*)     PREFERRED_PORT="$1" ;;
    *)          die "unknown argument: $1 (try --help)" ;;
  esac
  shift
done

case "$PREFERRED_PORT" in
  ''|*[!0-9]*) die "port must be a number, got: $PREFERRED_PORT" ;;
esac
[ "$PREFERRED_PORT" -ge 1 ] && [ "$PREFERRED_PORT" -le 65535 ] \
  || die "port out of range: $PREFERRED_PORT"

# --- prerequisites -----------------------------------------------------------

command -v cargo >/dev/null || die "cargo not found; install Rust from https://rustup.rs"

if [ "$DO_BUILD" -eq 1 ] && ! command -v cargo-leptos >/dev/null; then
  # The binary must be built by cargo-leptos: it sets LEPTOS_OUTPUT_NAME at
  # compile time, without which the client bundle URLs break and hydration dies.
  log "installing cargo-leptos (one-off)"
  cargo install cargo-leptos --locked
fi

# --- database ----------------------------------------------------------------

[ -f .env ] || {
  warn ".env missing, copying .env.example (no DataForSEO keys => Google fallback)"
  cp .env.example .env
}

# Read DATABASE_URL from .env without executing the file.
DB_URL="${DATABASE_URL:-$(sed -n 's/^[[:space:]]*DATABASE_URL[[:space:]]*=[[:space:]]*//p' .env | tail -n 1)}"
DB_URL="${DB_URL:-postgres://localhost/atp}"
DB_NAME="${DB_URL##*/}"
DB_NAME="${DB_NAME%%\?*}"

if command -v pg_isready >/dev/null && ! pg_isready -q 2>/dev/null; then
  warn "postgres does not look reachable; the app will fail to start"
  warn "try: brew services start postgresql@17"
fi

if command -v psql >/dev/null && [ -n "$DB_NAME" ]; then
  if ! psql -lqt 2>/dev/null | cut -d'|' -f1 | grep -qw "$DB_NAME"; then
    log "creating database $DB_NAME"
    createdb "$DB_NAME" || warn "could not create $DB_NAME; continuing anyway"
  fi
fi

# --- port selection ----------------------------------------------------------

port_in_use() {
  if command -v lsof >/dev/null; then
    lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1
  else
    # Fallback: bash's /dev/tcp probe. A successful connect means something listens.
    (exec 3<>"/dev/tcp/$HOST/$1") >/dev/null 2>&1 && { exec 3>&-; return 0; } || return 1
  fi
}

describe_occupant() {
  command -v lsof >/dev/null || { echo "unknown process"; return; }
  lsof -nP -iTCP:"$1" -sTCP:LISTEN -Fcp 2>/dev/null \
    | awk '/^p/{pid=substr($0,2)} /^c/{print substr($0,2)" (pid "pid")"; exit}'
}

# "Nearest free port" means nearest in either direction, not merely the next one
# upwards: with 3000 busy and 2999 free, 2999 is the closer answer. Ties prefer
# the higher port, since low ports are more likely to be privileged or reserved.
nearest_free_port() {
  base="$1"
  # Plain arithmetic rather than `seq`: BSD seq counts *down* when the end is
  # below the start, so `seq 1 0` yields "1 0" and a limit of 0 would still scan.
  offset=1
  while [ "$offset" -le "$MAX_PORT_TRIES" ]; do
    up=$((base + offset))
    down=$((base - offset))
    if [ "$up" -le 65535 ] && ! port_in_use "$up"; then
      echo "$up"; return 0
    fi
    # Stay out of the privileged range; binding there needs root.
    if [ "$down" -ge 1024 ] && ! port_in_use "$down"; then
      echo "$down"; return 0
    fi
    offset=$((offset + 1))
  done
  return 1
}

PORT_TO_USE="$PREFERRED_PORT"

if port_in_use "$PREFERRED_PORT"; then
  OCCUPANT="$(describe_occupant "$PREFERRED_PORT")"

  if [ "$KILL_OCCUPANT" -eq 1 ]; then
    log "port $PREFERRED_PORT held by ${OCCUPANT:-unknown}, stopping it (--kill)"
    lsof -ti:"$PREFERRED_PORT" -sTCP:LISTEN | xargs -r kill
    waited=0
    while [ "$waited" -lt 20 ]; do
      port_in_use "$PREFERRED_PORT" || break
      sleep 0.25
      waited=$((waited + 1))
    done
    port_in_use "$PREFERRED_PORT" && die "could not free port $PREFERRED_PORT"
  else
    warn "port $PREFERRED_PORT is taken by ${OCCUPANT:-unknown}"
    PORT_TO_USE="$(nearest_free_port "$PREFERRED_PORT")" \
      || die "no free port within $MAX_PORT_TRIES of $PREFERRED_PORT"
    log "using nearest free port: $PORT_TO_USE"
  fi
fi

ADDR="$HOST:$PORT_TO_USE"

# --- build -------------------------------------------------------------------

BIN="target/$PROFILE/atp"
if [ "$DO_BUILD" -eq 1 ]; then
  log "building ($PROFILE)"
  if [ "$PROFILE" = "release" ]; then
    cargo leptos build --release
  else
    cargo leptos build
  fi
else
  [ -x "$BIN" ] || die "$BIN not found; run without --no-build first"
fi

# cargo-leptos writes the server binary to target/<profile>/ or target/server/<profile>/
if [ ! -x "$BIN" ] && [ -x "target/server/$PROFILE/atp" ]; then
  BIN="target/server/$PROFILE/atp"
fi
[ -x "$BIN" ] || die "server binary not found (looked in target/$PROFILE and target/server/$PROFILE)"

# --- run ---------------------------------------------------------------------

# The port was free when we scanned, but building takes a while and something
# else may have grabbed it since. Re-check and slide over if needed.
if port_in_use "$PORT_TO_USE"; then
  warn "port $PORT_TO_USE was taken while building"
  PORT_TO_USE="$(nearest_free_port "$PORT_TO_USE")" \
    || die "no free port near $PORT_TO_USE"
  ADDR="$HOST:$PORT_TO_USE"
  log "moved to $PORT_TO_USE"
fi

# A stale browser tab pointing at the old port will happily talk to whatever
# took it over, and the resulting "error deserializing server function results"
# is baffling. Say plainly where this instance actually is.
if [ "$PORT_TO_USE" != "$PREFERRED_PORT" ]; then
  warn "NOTE: this instance is on $PORT_TO_USE, not $PREFERRED_PORT."
  warn "      A tab still open on $PREFERRED_PORT is talking to a different server."
fi

log "starting on http://$ADDR  (ctrl-c to stop)"
exec env \
  LEPTOS_SITE_ADDR="$ADDR" \
  LEPTOS_SITE_ROOT="${LEPTOS_SITE_ROOT:-target/site}" \
  "./$BIN"
