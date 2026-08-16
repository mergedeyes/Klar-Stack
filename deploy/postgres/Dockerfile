# Klar-Postgres mit vorkonfiguriertem TLS.
# Basiert auf dem offiziellen Image; der Wrapper-Entrypoint materialisiert
# Cert/Key, setzt die von Postgres verlangten Rechte und aktiviert ssl=on.
FROM postgres:18.6-trixie

# openssl für die Self-Signed-Fallback-Generierung (falls kein Cert per Env kommt).
RUN apt-get update \
 && apt-get install -y --no-install-recommends openssl \
 && rm -rf /var/lib/apt/lists/*

COPY docker-entrypoint-ssl.sh /usr/local/bin/docker-entrypoint-ssl.sh
RUN chmod +x /usr/local/bin/docker-entrypoint-ssl.sh

ENTRYPOINT ["docker-entrypoint-ssl.sh"]
CMD ["postgres"]
