# Backup-Sidecar für die Klar-Postgres-DB.
# Basiert bewusst auf demselben Postgres-Image wie der DB-Container,
# damit die pg_dump-Version exakt zur Server-Version passt.
FROM postgres:18.6-trixie

# awscli für den Upload zum Bunny Object Storage (S3-kompatibel, ausschließlich path-style).
RUN apt-get update \
 && apt-get install -y --no-install-recommends awscli ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY backup.sh /usr/local/bin/backup.sh
RUN chmod +x /usr/local/bin/backup.sh

# Das Postgres-Base-Image hat einen ENTRYPOINT, der einen DB-Server starten will.
# Wir wollen nur das Backup-Loop – ENTRYPOINT zurücksetzen.
ENTRYPOINT []
CMD ["/usr/local/bin/backup.sh"]
