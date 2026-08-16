# Backup-Sidecar für die Klar-Postgres-DB.
# Basiert bewusst auf demselben Postgres-Image wie der DB-Container,
# damit die pg_dump-Version exakt zur Server-Version passt.
FROM postgres:18.6-trixie

# awscli für den Upload zum Bunny Object Storage (S3-kompatibel, ausschließlich path-style).
RUN apt-get update \
 && apt-get install -y --no-install-recommends awscli ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Von Bunny automatisch erkannte Env-Variablen. Nicht-geheime Defaults sind
# gesetzt; Secrets (PGPASSWORD, AWS_*) und umgebungsspezifische Werte
# (S3_ENDPOINT, S3_BUCKET) füllst du im Dashboard.
ENV PGHOST=localhost \
    PGPORT=5432 \
    PGUSER=klar \
    PGPASSWORD="" \
    PGDATABASE=klar \
    S3_ENDPOINT="" \
    S3_BUCKET="" \
    S3_PREFIX=backups \
    AWS_ACCESS_KEY_ID="" \
    AWS_SECRET_ACCESS_KEY="" \
    RETENTION_DAYS=14 \
    BACKUP_INTERVAL_SECONDS=86400

COPY backup.sh /usr/local/bin/backup.sh
RUN chmod +x /usr/local/bin/backup.sh

# Das Postgres-Base-Image hat einen ENTRYPOINT, der einen DB-Server starten will.
# Wir wollen nur das Backup-Loop – ENTRYPOINT zurücksetzen.
ENTRYPOINT []
CMD ["/usr/local/bin/backup.sh"]
