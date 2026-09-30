"use client";

import { useEffect, useState, useSyncExternalStore } from "react";
import { useParams } from "next/navigation";
import { rightsApi, type RightsClaimStatus, type RightsClaimStatusView } from "@/lib/api";
import { Button } from "@/components/ui/button";

// The claimant's view of their rights claim. The token is the part of the
// link after "#": browsers never send it to a server, so it can't end up in
// an access log; it's only sent in a POST body.

const STATUS_DE: Record<RightsClaimStatus, { label: string; text: string }> = {
  submitted: { label: "Eingegangen", text: "Deine Meldung ist eingegangen und wartet auf die Prüfung." },
  triaged: { label: "In Prüfung", text: "Unser Team prüft deine Meldung." },
  evidence_requested: { label: "Rückfrage", text: "Wir brauchen noch Angaben von dir, siehe unten." },
  accepted: {
    label: "Angenommen",
    text: "Wir haben den Beitrag ausgeblendet. Die Person, die ihn veröffentlicht hat, kann widersprechen; über das Ergebnis informieren wir dich.",
  },
  declined: { label: "Abgelehnt", text: "Wir haben den Beitrag nach Prüfung nicht ausgeblendet. Die Begründung steht unten." },
  restored: {
    label: "Wiederhergestellt",
    text: "Der Widerspruch gegen die Ausblendung hatte Erfolg; der Beitrag ist wieder sichtbar. Dir stehen der Rechtsweg und die außergerichtliche Streitbeilegung offen.",
  },
};

function subscribeToHash(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

export default function RightsClaimStatusPage() {
  const { id } = useParams<{ id: string }>();
  // "" on the server and during hydration, then the fragment from the URL.
  const token = useSyncExternalStore(subscribeToHash, () => window.location.hash.slice(1), () => "");
  const [claim, setClaim] = useState<RightsClaimStatusView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [response, setResponse] = useState("");
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!token) return;
    rightsApi.status(id, token)
      .then(setClaim)
      .catch(() => setError("Diese Meldung wurde nicht gefunden. Prüfe, ob du den vollständigen Link aus deiner Bestätigungs-E-Mail verwendest."));
  }, [id, token]);

  const respond = async () => {
    setSending(true);
    setError(null);
    try {
      setClaim(await rightsApi.respond(id, token, response));
      setResponse("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Die Antwort konnte nicht gesendet werden");
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <main className="mx-auto max-w-xl px-4 py-8">
        <h1 className="mb-4 text-2xl font-bold">Deine Rechte-Meldung</h1>

        {!token ? (
          <p className="text-sm text-muted-foreground">
            Öffne diese Seite über den persönlichen Link aus deiner Bestätigungs-E-Mail — nur mit ihm können wir dir
            den Stand zeigen.
          </p>
        ) : error && !claim ? (
          <p className="text-sm text-destructive">{error}</p>
        ) : !claim ? (
          <p className="text-sm text-muted-foreground animate-pulse">Lädt…</p>
        ) : (
          <div className="space-y-4">
            <section className="rounded-xl border border-border p-4">
              <p className="text-sm text-muted-foreground">Stand</p>
              <p className="font-semibold">{STATUS_DE[claim.status].label}</p>
              <p className="mt-1 text-sm">{STATUS_DE[claim.status].text}</p>
            </section>

            <section className="space-y-2 rounded-xl border border-border p-4 text-sm">
              <p><span className="text-muted-foreground">Eingegangen am </span>{new Date(claim.created_at).toLocaleString("de-DE")}</p>
              <p className="break-all"><span className="text-muted-foreground">Beitrag: </span>{claim.content_url}</p>
              <p className="whitespace-pre-wrap"><span className="text-muted-foreground">Werk: </span>{claim.work_description}</p>
            </section>

            {claim.evidence_request && (
              <section className="space-y-2 rounded-xl border border-border p-4 text-sm">
                <p className="font-semibold">Unsere Rückfrage</p>
                <p className="whitespace-pre-wrap">{claim.evidence_request}</p>
                {claim.status === "evidence_requested" ? (
                  <form onSubmit={(e) => { e.preventDefault(); respond(); }} className="space-y-2 pt-2">
                    <textarea
                      value={response}
                      onChange={(e) => setResponse(e.target.value)}
                      maxLength={4000}
                      rows={4}
                      placeholder="Deine Antwort"
                      className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                    />
                    {error && <p className="text-destructive">{error}</p>}
                    <Button type="submit" size="sm" disabled={sending || !response.trim()}>Antwort senden</Button>
                  </form>
                ) : (
                  claim.claimant_response && (
                    <>
                      <p className="pt-2 font-semibold">Deine Antwort</p>
                      <p className="whitespace-pre-wrap">{claim.claimant_response}</p>
                    </>
                  )
                )}
              </section>
            )}

            {claim.decision_reason && (
              <section className="space-y-2 rounded-xl border border-border p-4 text-sm">
                <p className="font-semibold">Begründung</p>
                <p className="whitespace-pre-wrap">{claim.decision_reason}</p>
              </section>
            )}
          </div>
        )}
      </main>
    </div>
  );
}
