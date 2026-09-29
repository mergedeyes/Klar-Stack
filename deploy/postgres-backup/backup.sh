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
: "${BACKUP_TIME:=03:00}"                      # HH:MM, UTC
: "${BACKUP_RETRY_SECONDS:=3600}"              # Wartezeit nach einem Fehlschlag
: "${BACKUP_TOLERANCE_MINUTES:=20}"            # Toleranz um BACKUP_TIME, s. backup_due

if ! [[ "${BACKUP_TIME}" =~ ^([01][0-9]|2[0-3]):[0-5][0-9]$ ]]; then
  echo "BACKUP_TIME muss HH:MM (UTC) sein, ist aber '${BACKUP_TIME}'" >&2
  exit 1
fi

# --- Alerting (optional, aber dringend empfohlen) ---
# Dead-Man's-Switch im Healthchecks.io-Format: nach jedem erfolgreichen
# Backup wird HEALTHCHECK_URL angepingt, bei einem Fehler HEALTHCHECK_URL/fail.
# Der Dienst alarmiert, wenn ein Ping ausbleibt -- das deckt auch den Fall ab,
# dass dieser Container gar nicht mehr läuft oder schon beim Start scheitert,
# den das Skript selbst nie melden könnte. (Genau so ist der Ausfall vom
# 24.08.-29.09.2026 fünf Wochen lang unbemerkt geblieben.)
# Es werden keine Daten übertragen, nur der Ping selbst.
: "${HEALTHCHECK_URL:=}"

export PGHOST PGPORT PGUSER PGPASSWORD PGDATABASE
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY

# Bunny akzeptiert ausschließlich path-style Adressierung.
aws configure set default.s3.addressing_style path

log() { echo "[$(date -u +%FT%TZ)] $*"; }

# $1: "" für Erfolg, "/fail" für Fehler. Ein fehlgeschlagener Ping bricht
# nichts ab -- bleibt er aus, schlägt der Dienst ohnehin Alarm.
ping_healthcheck() {
  [ -z "${HEALTHCHECK_URL}" ] && return 0
  curl -fsS -m 10 --retry 3 -o /dev/null "${HEALTHCHECK_URL%/}$1" \
    || log "Healthcheck-Ping fehlgeschlagen" >&2
}

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

# Prints the timestamps (YYYYMMDDTHHMMSSZ) of all dumps in the bucket, one
# per line and sorted. A failed listing prints nothing, which callers treat
# as "no backup exists" -- erring towards an extra dump rather than a gap.
list_backup_ts() {
  aws --endpoint-url "${S3_ENDPOINT}" s3api list-objects-v2 \
        --bucket "${S3_BUCKET}" --prefix "${S3_PREFIX}/" \
        --query "Contents[].Key" --output text 2>/dev/null \
    | tr '\t' '\n' \
    | sed -n 's/.*-\([0-9]\{8\}T[0-9]\{6\}Z\)\.dump$/\1/p' \
    | sort || true
}

# Epoch seconds of the most recent BACKUP_TIME that is not in the future,
# i.e. today's slot once it has passed, otherwise yesterday's.
last_slot_epoch() {
  local now slot
  now="$(date -u +%s)"
  slot="$(date -u -d "$(date -u +%F) ${BACKUP_TIME} UTC" +%s)"
  if [ "${now}" -lt "${slot}" ]; then
    slot=$((slot - 86400))
  fi
  echo "${slot}"
}

# A backup is due when the bucket holds nothing taken at or after the last
# slot. Checking the bucket instead of keeping local state means restarts and
# redeploys no longer produce extra dumps, while a slot that was missed
# (container down at 03:00, or the run failed) is caught up right away.
# A dump from up to BACKUP_TOLERANCE_MINUTES before the slot also counts, so
# a catch-up run shortly before 03:00 isn't immediately followed by another.
backup_due() {
  local slot_ts latest
  slot_ts="$(date -u -d "@$(( $(last_slot_epoch) - BACKUP_TOLERANCE_MINUTES * 60 ))" +%Y%m%dT%H%M%SZ)"
  latest="$(list_backup_ts | tail -n 1)"
  if [ -n "${latest}" ] && [[ ! "${latest}" < "${slot_ts}" ]]; then
    log "Backup für Slot ${slot_ts} vorhanden (${latest}), überspringe"
    return 1
  fi
  return 0
}

sleep_until_next_slot() {
  local next secs
  next=$(( $(last_slot_epoch) + 86400 ))
  secs=$(( next - $(date -u +%s) ))
  log "Nächstes Backup um $(date -u -d "@${next}" +%FT%TZ)"
  sleep "${secs}"
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

log "Backup-Sidecar gestartet (täglich ${BACKUP_TIME} UTC, Retention ${RETENTION_DAYS}d)"
[ -n "${HEALTHCHECK_URL}" ] || log "WARNUNG: HEALTHCHECK_URL nicht gesetzt -- fehlschlagende Backups bleiben unbemerkt" >&2
while true; do
  if backup_due; then
    if run_backup; then
      log "Backup ok"
      ping_healthcheck ""
    else
      log "Backup FEHLGESCHLAGEN, neuer Versuch in ${BACKUP_RETRY_SECONDS}s" >&2
      ping_healthcheck "/fail"
      sleep "${BACKUP_RETRY_SECONDS}"
      continue
    fi
  fi
  sleep_until_next_slot
done
