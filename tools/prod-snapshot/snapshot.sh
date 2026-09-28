#!/usr/bin/env bash
# Anonymized local copy of the production database, built from the latest
# nightly backup. See README.md in this directory.
#
#   ./snapshot.sh refresh [--from-file DUMP]   rebuild from the latest backup
#   ./snapshot.sh backups                      list the backups in storage
#   ./snapshot.sh psql [ARGS...]               open psql (or run: psql -c '...')
#   ./snapshot.sh checks                       run every checks/*.sql
#   ./snapshot.sh migrate                      apply this checkout's pending migrations
#   ./snapshot.sh url                          print the DATABASE_URL
#   ./snapshot.sh drop                         delete the snapshot
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "${HERE}/../.." && pwd)"

CONTAINER="klar-snapshot"
PG_IMAGE="postgres:18.6-trixie"        # same as deploy/postgres
AWS_IMAGE="amazon/aws-cli:2.27.50"
PORT="${SNAPSHOT_PORT:-55432}"
PGPASS="snapshot"                      # local only, bound to 127.0.0.1
DB="klar"
IMPORT_DB="klar_import"                # raw data only ever lives here, briefly

# Set while the raw (not yet anonymized) import exists; the EXIT trap
# drops it on any failure, Ctrl-C included.
RAW_IMPORT_LIVE=0
drop_raw_import() {
    if [ "${RAW_IMPORT_LIVE}" = 1 ]; then
        printf '\033[31mfailed -- dropping the raw import\033[0m\n' >&2
        docker exec -e PGPASSWORD="${PGPASS}" "${CONTAINER}" \
            psql -U postgres -q -c "DROP DATABASE IF EXISTS ${IMPORT_DB} WITH (FORCE)" >/dev/null 2>&1 || true
    fi
}
trap drop_raw_import EXIT
trap 'exit 130' INT TERM

log()  { printf '\033[1m==>\033[0m %s\n' "$*"; }
die()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

in_pg() { docker exec -i -e PGPASSWORD="${PGPASS}" "${CONTAINER}" "$@"; }
psql_db() { local db="$1"; shift; in_pg psql -U postgres -d "${db}" -v ON_ERROR_STOP=1 "$@"; }

require_running() {
    [ "$(docker inspect -f '{{.State.Running}}' "${CONTAINER}" 2>/dev/null)" = "true" ] \
        || die "no snapshot running -- run: $0 refresh"
}

load_env() {
    local env_file="${HERE}/snapshot.env"
    [ -f "${env_file}" ] || die "missing ${env_file} (copy snapshot.env.example and fill it in)"
    # shellcheck disable=SC1090
    set -a; source "${env_file}"; set +a
    # Bunny's S3 API: bucket and access key id are both the storage zone
    # name, the secret is the zone password. The AWS_*/S3_BUCKET names are
    # still accepted, e.g. copied straight from the backup sidecar's env.
    : "${BUNNY_STORAGE_ZONE:=${S3_BUCKET:-}}"
    : "${BUNNY_STORAGE_PASSWORD:=${AWS_SECRET_ACCESS_KEY:-}}"
    [ -n "${BUNNY_STORAGE_ZONE}" ]     || die "BUNNY_STORAGE_ZONE missing in snapshot.env"
    [ -n "${BUNNY_STORAGE_PASSWORD}" ] || die "BUNNY_STORAGE_PASSWORD missing in snapshot.env"
    S3_BUCKET="${BUNNY_STORAGE_ZONE}"
    AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-${BUNNY_STORAGE_ZONE}}"
    AWS_SECRET_ACCESS_KEY="${BUNNY_STORAGE_PASSWORD}"
    export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
    : "${S3_ENDPOINT:=https://de-s3.storage.bunnycdn.com}"
    : "${S3_PREFIX:=backups}"
}

# Runs the AWS CLI in a container, configured like the backup sidecar
# (deploy/postgres-backup/backup.sh): Bunny only supports path-style.
aws_cli() {
    docker run --rm -i \
        -e AWS_ACCESS_KEY_ID -e AWS_SECRET_ACCESS_KEY -e AWS_DEFAULT_REGION=us-east-1 \
        --entrypoint sh "${AWS_IMAGE}" -c \
        'aws configure set default.s3.addressing_style path && exec aws --endpoint-url "$0" "$@"' \
        "${S3_ENDPOINT}" "$@"
}

# Prints "<timestamp> <key> <bytes>" per backup, oldest first. Sorted by
# the UTC timestamp embedded in the name (klar-<db>-YYYYMMDDTHHMMSSZ.dump),
# not by the whole key: the name also contains the database name, and
# after a rename (e.g. klar-db -> klar) sorting whole keys would put old
# "klar-klar-db-..." backups after new "klar-klar-2026..." ones.
list_backups() {
    aws_cli s3api list-objects-v2 --bucket "${S3_BUCKET}" --prefix "${S3_PREFIX}/" \
        --query 'Contents[].[Key, Size]' --output text \
        | awk 'match($1, /[0-9]{8}T[0-9]{6}Z\.dump$/) { print substr($1, RSTART, 16), $1, $2 }' \
        | sort
}

ts_epoch() { date -u -d "${1:0:8} ${1:9:2}:${1:11:2}:${1:13:2}" +%s; }

# Age of a backup timestamp in whole hours.
ts_age_hours() { echo $(( ($(date -u +%s) - $(ts_epoch "$1")) / 3600 )); }

# The nightly sidecar should never leave the newest backup older than this.
STALE_AFTER_HOURS=48

warn_if_stale() {
    local ts="$1" age
    age="$(ts_age_hours "${ts}")"
    if [ "${age}" -gt "${STALE_AFTER_HOURS}" ]; then
        printf '\033[33mwarning:\033[0m the newest backup is %s days old -- the db-backup sidecar has likely\n         stopped producing backups. Check its logs in the Bunny dashboard.\n' "$(( age / 24 ))" >&2
    fi
}

start_container() {
    if docker inspect "${CONTAINER}" >/dev/null 2>&1; then
        log "Removing the previous snapshot"
        docker rm -f "${CONTAINER}" >/dev/null
    fi
    log "Starting ${PG_IMAGE} on 127.0.0.1:${PORT}"
    # Data directory on tmpfs: the snapshot lives in RAM only and is gone
    # when the container stops -- nothing is ever written to disk.
    docker run -d --name "${CONTAINER}" \
        -e POSTGRES_PASSWORD="${PGPASS}" \
        -p "127.0.0.1:${PORT}:5432" \
        --tmpfs /var/lib/postgresql:rw,size=4g \
        "${PG_IMAGE}" >/dev/null
    for _ in $(seq 1 60); do
        in_pg pg_isready -U postgres -q 2>/dev/null && return 0
        sleep 1
    done
    die "Postgres did not become ready"
}

cmd_refresh() {
    local from_file=""
    if [ "${1:-}" = "--from-file" ]; then
        from_file="${2:?--from-file needs a path}"
        [ -f "${from_file}" ] || die "no such file: ${from_file}"
    fi

    local key=""
    if [ -z "${from_file}" ]; then
        load_env
        log "Looking up the latest backup in s3://${S3_BUCKET}/${S3_PREFIX}/"
        local latest ts
        latest="$(list_backups | tail -n 1)"
        [ -n "${latest}" ] || die "no backups found"
        read -r ts key _ <<< "${latest}"
        log "Latest: ${key} ($(( $(ts_age_hours "${ts}") / 24 )) days old)"
        warn_if_stale "${ts}"
    fi

    start_container
    RAW_IMPORT_LIVE=1
    psql_db postgres -q -c "CREATE DATABASE ${IMPORT_DB}"

    log "Restoring (streamed, the dump is never written to disk)"
    if [ -n "${from_file}" ]; then
        in_pg pg_restore -U postgres -d "${IMPORT_DB}" --no-owner --no-privileges --exit-on-error < "${from_file}"
    else
        aws_cli s3 cp "s3://${S3_BUCKET}/${key}" - \
            | in_pg pg_restore -U postgres -d "${IMPORT_DB}" --no-owner --no-privileges --exit-on-error
    fi

    log "Pre-flight checks on the real data (counts only)"
    psql_db "${IMPORT_DB}" -q < "${HERE}/preflight.sql"

    log "Anonymizing"
    psql_db "${IMPORT_DB}" -q --single-transaction < "${HERE}/anonymize.sql"

    # Only the anonymized database is ever called "${DB}".
    psql_db postgres -q -c "ALTER DATABASE ${IMPORT_DB} RENAME TO ${DB}"
    RAW_IMPORT_LIVE=0

    log "Snapshot ready${key:+ (from ${key})}"
    echo "    DATABASE_URL=$(cmd_url)"
    echo "    every account's password: klar-dev-password"
    echo "    next: $0 checks | $0 migrate | $0 psql"
}

cmd_psql() {
    require_running
    if [ -t 0 ] && [ $# -eq 0 ]; then
        docker exec -it -e PGPASSWORD="${PGPASS}" "${CONTAINER}" psql -U postgres -d "${DB}"
    else
        psql_db "${DB}" "$@"
    fi
}

cmd_checks() {
    require_running
    local f
    for f in "${HERE}"/checks/*.sql; do
        log "$(basename "${f}")"
        psql_db "${DB}" -q < "${f}"
    done
}

cmd_migrate() {
    require_running
    command -v cargo >/dev/null || die "cargo not found (needs sqlx-cli: cargo install sqlx-cli)"
    log "Applying pending migrations from ${REPO}/Klar/migrations"
    (cd "${REPO}/Klar" && DATABASE_URL="$(cmd_url)" cargo sqlx migrate run)
    log "Warnings raised by migrations, if any, are in the server log:"
    docker logs "${CONTAINER}" 2>&1 | grep -E 'WARNING' | tail -n 20 || echo "    (none)"
}

cmd_backups() {
    load_env
    local rows ts key bytes n=0
    rows="$(list_backups)"
    [ -n "${rows}" ] || die "no backups found in s3://${S3_BUCKET}/${S3_PREFIX}/"
    printf '%-20s %9s  %s\n' "TAKEN (UTC)" "SIZE" "KEY"
    while read -r ts key bytes; do
        n=$((n + 1))
        printf '%-20s %7s K  %s\n' "$(date -u -d "@$(ts_epoch "${ts}")" '+%F %H:%M')" "$(( bytes / 1024 ))" "${key}"
    done <<< "${rows}"
    ts="$(tail -n 1 <<< "${rows}" | cut -d' ' -f1)"
    log "${n} backups, newest $(( $(ts_age_hours "${ts}") / 24 )) days old"
    warn_if_stale "${ts}"
}

cmd_url()  { echo "postgres://postgres:${PGPASS}@127.0.0.1:${PORT}/${DB}"; }

cmd_drop() {
    docker rm -f "${CONTAINER}" >/dev/null 2>&1 && log "Snapshot deleted" || log "No snapshot to delete"
}

case "${1:-}" in
    refresh) shift; cmd_refresh "$@" ;;
    backups) cmd_backups ;;
    psql)    shift; cmd_psql "$@" ;;
    checks)  cmd_checks ;;
    migrate) cmd_migrate ;;
    url)     cmd_url ;;
    drop)    cmd_drop ;;
    *) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 1 ;;
esac
