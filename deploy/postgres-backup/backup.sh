#!/usr/bin/env bash
# Nächtliches Backup der Klar-Postgres-DB nach Bunny Object Storage.
# Läuft als Sidecar im selben Pod wie der Postgres-Container und erreicht
# ihn über localhost. Alle Werte kommen aus Container-Env-Variablen.
set -euo pipefail

# --- DB-Verbindung (localhost, da gleicher Pod) ---
: "${PGHOST:=localhost}"
: "${PGPORT:=5432}"
: "${PGUSER:?PGUSER fehlt}"
: "${PGPASSWORD:?PGPASSWORD fehlt}"
: "${PGDATABASE:?PGDATABASE fehlt}"

# --- Bunny Object Storage (S3-Gateway, path-style) ---
: "${S3_ENDPOINT:?S3_ENDPOINT fehlt}"          # z.B. https://storage.bunnycdn.com
: "${S3_BUCKET:?S3_BUCKET fehlt}"
: "${AWS_ACCESS_KEY_ID:?AWS_ACCESS_KEY_ID fehlt}"
: "${AWS_SECRET_ACCESS_KEY:?AWS_SECRET_ACCESS_KEY fehlt}"
: "${S3_PREFIX:=backups}"
: "${RETENTION_DAYS:=14}"
: "${BACKUP_INTERVAL_SECONDS:=86400}"          # 24h

export PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

# Bunny akzeptiert ausschließlich path-style Adressierung.
aws configure set default.s3.addressing_style path

log() { echo "[$(date -u +%FT%TZ)] $*"; }

run_backup() {
  local ts file key
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  file="/tmp/klar-${PGDATABASE}-${ts}.dump"
  key="${S3_PREFIX}/klar-${PGDATABASE}-${ts}.dump"

  log "pg_dump -> ${file}"
  # -Fc  = Custom-Format (komprimiert, für pg_restore)
  # --no-owner / --no-privileges halten den Dump für Restores in eine frische DB portabel.
  #
  # NOTE: this function is invoked as `if run_backup; then …` below, and
  # bash's `set -e` does NOT propagate inside the condition of an
  # if/while/until — that suppression applies to every command run as
  # part of evaluating the condition, including a whole function called
  # from it. So each step here checks its own exit status explicitly.
  # A bare `pg_dump ...` relying on set -e in this context would silently
  # continue past a failed dump and upload whatever partial (possibly
  # empty) file pg_dump left behind — which is exactly what was happening:
  # a failing pg_dump left a 0-byte file that still got shipped to S3 and
  # treated as a valid backup.
  if ! pg_dump -Fc --no-owner --no-privileges -f "${file}"; then
    log "pg_dump FEHLGESCHLAGEN, kein Upload" >&2
    rm -f "${file}"
    return 1
  fi

  # Belt-and-suspenders: refuse to upload anything that isn't at least a
  # plausible custom-format dump. Custom-format archives start with a
  # 5-byte "PGDMP" magic header; anything under ~512 bytes for a live
  # social-network DB is definitely not a real dump.
  if [ ! -s "${file}" ] || [ "$(stat -c%s "${file}")" -lt 512 ]; then
    log "pg_dump lieferte eine verdächtig kleine/leere Datei (${file}), kein Upload" >&2
    rm -f "${file}"
    return 1
  fi

  log "Upload -> s3://${S3_BUCKET}/${key}"
  if ! aws --endpoint-url "${S3_ENDPOINT}" s3 cp "${file}" "s3://${S3_BUCKET}/${key}"; then
    log "Upload FEHLGESCHLAGEN" >&2
    rm -f "${file}"
    return 1
  fi
  rm -f "${file}"

  prune_old
}

prune_old() {
  # Retention über den im Dateinamen kodierten Zeitstempel (YYYYMMDDTHHMMSSZ),
  # nicht über S3 LastModified – das umgeht Format-Fallstricke beim Datumsvergleich.
  local cutoff_ts
  cutoff_ts="$(date -u -d "-${RETENTION_DAYS} days" +%Y%m%dT%H%M%SZ)"
  log "Prune älter als ${cutoff_ts}"
  aws --endpoint-url "${S3_ENDPOINT}" s3api list-objects-v2 \
        --bucket "${S3_BUCKET}" --prefix "${S3_PREFIX}/" \
        --query "Contents[].Key" --output text 2>/dev/null \
    | tr '\t' '\n' | while read -r k; do
        [ -z "${k}" ] && continue
        local kts
        kts="$(printf '%s' "${k}" | sed -n 's/.*-\([0-9]\{8\}T[0-9]\{6\}Z\)\.dump$/\1/p')"
        [ -z "${kts}" ] && continue
        if [[ "${kts}" < "${cutoff_ts}" ]]; then
          log "  lösche ${k}"
          aws --endpoint-url "${S3_ENDPOINT}" s3 rm "s3://${S3_BUCKET}/${k}"
        fi
      done
}

log "Backup-Sidecar gestartet (Intervall ${BACKUP_INTERVAL_SECONDS}s, Retention ${RETENTION_DAYS}d)"
while true; do
  if run_backup; then
    log "Backup ok"
  else
    log "Backup FEHLGESCHLAGEN" >&2
  fi
  sleep "${BACKUP_INTERVAL_SECONDS}"
done
