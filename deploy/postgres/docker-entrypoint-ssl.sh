#!/usr/bin/env bash
# Wrapper um den offiziellen Postgres-Entrypoint: sorgt für ein Server-Cert/Key
# mit korrekten Rechten und startet Postgres mit ssl=on. Läuft als root
# (der offizielle Entrypoint dropt danach selbst via gosu auf den postgres-User).
set -euo pipefail

SSL_DIR=/etc/postgresql/ssl
mkdir -p "${SSL_DIR}"

if [ -n "${POSTGRES_SSL_CERT:-}" ] && [ -n "${POSTGRES_SSL_KEY:-}" ]; then
  # Echtes Cert/Key aus Env (z.B. für einen späteren B-Split mit verify-full).
  printf '%s\n' "${POSTGRES_SSL_CERT}" > "${SSL_DIR}/server.crt"
  printf '%s\n' "${POSTGRES_SSL_KEY}"  > "${SSL_DIR}/server.key"
else
  # Fallback: self-signed. Reicht für sslmode=require (Verschlüsselung ohne
  # Verifikation), was für die Loopback-Verbindung im selben Pod das Richtige ist.
  if [ ! -f "${SSL_DIR}/server.key" ]; then
    echo "Kein POSTGRES_SSL_CERT/KEY gesetzt – erzeuge self-signed Zertifikat."
    openssl req -new -x509 -days 3650 -nodes \
      -subj "/CN=klar-db" \
      -out "${SSL_DIR}/server.crt" -keyout "${SSL_DIR}/server.key"
  fi
fi

# Postgres verweigert den Start, wenn der Key zu offen liegt oder falsch gehört.
chown postgres:postgres "${SSL_DIR}/server.crt" "${SSL_DIR}/server.key"
chmod 600 "${SSL_DIR}/server.key"
chmod 644 "${SSL_DIR}/server.crt"

# An den offiziellen Entrypoint weiterreichen, TLS-Parameter angehängt.
exec docker-entrypoint.sh postgres \
  -c ssl=on \
  -c ssl_cert_file="${SSL_DIR}/server.crt" \
  -c ssl_key_file="${SSL_DIR}/server.key" \
  "${@:2}"
