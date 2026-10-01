"use client";

import { useEffect, useState, useSyncExternalStore } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { noticesApi, type NoticeStatus, type ReportOutcome } from "@/lib/api";

// The notifier's view of their notice about illegal content. The token is
// the part of the link after "#": browsers never send it to a server, so it
// can't end up in an access log; it's only sent in a POST body.

const OUTCOME_DE: Record<ReportOutcome, string> = {
  removed: "Wir haben den gemeldeten Inhalt geprüft und entfernt.",
  account_measure: "Wir haben den gemeldeten Inhalt geprüft und Maßnahmen gegen das Konto ergriffen.",
  no_violation:
    "Wir haben den gemeldeten Inhalt geprüft und keinen Verstoß gegen das Gesetz oder unsere Nutzungsbedingungen festgestellt. Er bleibt deshalb sichtbar.",
  obsolete: "Der gemeldete Inhalt wurde gelöscht, bevor wir ihn prüfen konnten. Er ist nicht mehr verfügbar.",
  duplicate: "Diese Meldung wurde mit einer gleichen Meldung zusammengefasst.",
};

function subscribeToHash(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

export default function NoticeStatusPage() {
  const { id } = useParams<{ id: string }>();
  // "" on the server and during hydration, then the fragment from the URL.
  const token = useSyncExternalStore(subscribeToHash, () => window.location.hash.slice(1), () => "");
  const [notice, setNotice] = useState<NoticeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) return;
    noticesApi.status(id, token)
      .then(setNotice)
      .catch(() => setError("Diese Meldung wurde nicht gefunden. Prüfe, ob du den vollständigen Link aus deiner Bestätigung verwendest."));
  }, [id, token]);

  return (
    <div className="flex-1 bg-background">
      <main className="mx-auto max-w-xl px-4 py-8">
        <h1 className="mb-4 text-2xl font-bold">Deine Meldung</h1>

        {!token ? (
          <p className="text-sm text-muted-foreground">
            Öffne diese Seite über den persönlichen Link aus deiner Bestätigung — nur mit ihm können wir dir den Stand
            zeigen. Unsere Entscheidung haben wir dir außerdem per E-Mail geschickt, falls du eine Adresse angegeben hast.
          </p>
        ) : error ? (
          <p className="text-sm text-destructive">{error}</p>
        ) : !notice ? (
          <p className="text-sm text-muted-foreground animate-pulse">Lädt…</p>
        ) : (
          <div className="space-y-4">
            <section className="rounded-xl border border-border p-4">
              <p className="text-sm text-muted-foreground">Stand</p>
              <p className="font-semibold">{notice.outcome ? "Entschieden" : "In Prüfung"}</p>
              <p className="mt-1 text-sm">
                {notice.outcome
                  ? OUTCOME_DE[notice.outcome]
                  : "Unser Team prüft deine Meldung. Über die Entscheidung informieren wir dich hier und per E-Mail."}
              </p>
              {notice.decided_at && (
                <p className="mt-1 text-xs text-muted-foreground">
                  Entschieden am {new Date(notice.decided_at).toLocaleString("de-DE")}
                </p>
              )}
            </section>
            {notice.outcome && (
              <p className="text-sm text-muted-foreground">
                Bist du mit der Entscheidung nicht einverstanden, schreib uns an{" "}
                <a href="mailto:kontakt@klarsocial.eu" className="underline">kontakt@klarsocial.eu</a>. Dir stehen außerdem
                eine außergerichtliche Streitbeilegung (Art. 21 DSA) und der Rechtsweg offen.
              </p>
            )}
            <section className="rounded-xl border border-border p-4 text-sm">
              <p className="text-muted-foreground">Gemeldet am {new Date(notice.created_at).toLocaleString("de-DE")}</p>
              <p className="mt-1 break-all">{notice.content_url}</p>
              <p className="mt-2 whitespace-pre-wrap">{notice.explanation}</p>
            </section>
            <Link href="/notices" className="text-sm underline">Weitere Meldung</Link>
          </div>
        )}
      </main>
    </div>
  );
}
