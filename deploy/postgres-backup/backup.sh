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
  pg_dump -Fc --no-owner --no-privileges -f "${file}"

  log "Upload -> s3://${S3_BUCKET}/${key}"
  aws --endpoint-url "${S3_ENDPOINT}" s3 cp "${file}" "s3://${S3_BUCKET}/${key}"
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
