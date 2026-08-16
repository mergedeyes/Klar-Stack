# Klar-Postgres mit vorkonfiguriertem TLS.
# Basiert auf dem offiziellen Image; der Wrapper-Entrypoint materialisiert
# Cert/Key, setzt die von Postgres verlangten Rechte und aktiviert ssl=on.
FROM postgres:18.6-trixie

# openssl für die Self-Signed-Fallback-Generierung (falls kein Cert per Env kommt).
RUN apt-get update \
 && apt-get install -y --no-install-recommends openssl \
 && rm -rf /var/lib/apt/lists/*

# Von Bunny automatisch erkannte Env-Variablen. Nicht-geheime Defaults sind
# gesetzt; das leere POSTGRES_PASSWORD füllst du im Dashboard (leer = lauter
# Fehler beim Start, kein stiller Default).
ENV POSTGRES_USER=klar \
    POSTGRES_DB=klar \
    POSTGRES_HOST_AUTH_METHOD=scram-sha-256 \
    POSTGRES_PASSWORD=""

COPY docker-entrypoint-ssl.sh /usr/local/bin/docker-entrypoint-ssl.sh
RUN chmod +x /usr/local/bin/docker-entrypoint-ssl.sh

ENTRYPOINT ["docker-entrypoint-ssl.sh"]
CMD ["postgres"]
