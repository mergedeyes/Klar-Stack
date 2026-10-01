"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { moderationApi, type ModerationDecision } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

// The statement of reasons (DSA Art. 17) for one decision. In German like
// the legal pages and the notification email, since it's the formal notice.
// ⚖️ Wording pending legal review.

const RESTRICTION_DE: Record<ModerationDecision["restriction"], string> = {
  removed: "Entfernt",
  hidden: "Ausgeblendet",
  flagged: "Nur mit Warnhinweis angezeigt",
  warning: "Verwarnt",
  suspended: "Vorübergehend gesperrt",
  banned: "Dauerhaft gesperrt",
};

const TARGET_DE: Record<ModerationDecision["target_type"], string> = {
  post: "Beitrag",
  comment: "Kommentar",
  user: "Konto",
  message: "Nachricht",
};

// What the decision followed (Art. 17(3)(b) DSA).
const SOURCE_DE: Record<NonNullable<ModerationDecision["source"]>, string> = {
  notice: "Eine Meldung",
  own_initiative: "Eine eigene Prüfung unseres Teams, ohne Meldung",
  authority_order: "Eine behördliche Anordnung",
  rights_claim: "Eine Meldung wegen Verletzung von Rechten (Rechte-Meldung)",
};

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-1 sm:grid-cols-[10rem_1fr] sm:gap-3">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="min-w-0 break-words text-sm">{children}</dd>
    </div>
  );
}

export default function DecisionPage() {
  const { id } = useParams<{ id: string }>();
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [decision, setDecision] = useState<ModerationDecision | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [objection, setObjection] = useState("");
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    moderationApi.decision(id)
      .then(setDecision)
      .catch((err) => setError(err instanceof Error ? err.message : "Nicht gefunden"));
  }, [user, id]);

  const submitObjection = async () => {
    setSending(true);
    setError(null);
    try {
      setDecision(await moderationApi.object(id, objection));
      setObjection("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Widerspruch konnte nicht gesendet werden");
    } finally {
      setSending(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background pb-12">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Zurück" />
        <span className="font-semibold">Moderationsentscheidung</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {!decision ? (
          !error && <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Lädt…</div>
        ) : (
          <>
            <section className="mb-4 rounded-xl border border-border p-4">
              <dl className="space-y-3">
                <Row label="Betroffen">
                  Dein {TARGET_DE[decision.target_type]}
                  {decision.content_excerpt && (
                    <span className="mt-1 block whitespace-pre-wrap rounded bg-muted/50 p-2 text-muted-foreground">
                      {decision.content_excerpt}
                    </span>
                  )}
                </Row>
                <Row label="Maßnahme">
                  {RESTRICTION_DE[decision.restriction]} am {new Date(decision.created_at).toLocaleString("de-DE")}
                  {decision.lifted_at && (
                    <span className="block text-muted-foreground">
                      Aufgehoben am {new Date(decision.lifted_at).toLocaleString("de-DE")}
                    </span>
                  )}
                  {decision.superseded && (
                    <span className="block text-muted-foreground">
                      Durch eine spätere Entscheidung ersetzt
                      {decision.superseded_by && (
                        <>
                          {" — "}
                          <Link href={`/moderation/decisions/${decision.superseded_by}`} className="underline">
                            zur neuen Begründung
                          </Link>
                        </>
                      )}
                    </span>
                  )}
                </Row>
                {decision.source && <Row label="Anlass">{SOURCE_DE[decision.source]}</Row>}
                <Row label="Entschieden">
                  {decision.automated
                    ? "Automatisch, unmittelbar nach einer Meldung — noch ohne Prüfung durch unser Team"
                    : "Von unserem Moderationsteam nach Prüfung"}
                </Row>
                <Row label="Grundlage">
                  {decision.ground_type === "illegal" ? "Mutmaßlich rechtswidriger Inhalt: " : "Verstoß gegen unsere "}
                  {decision.ground_type === "terms" ? (
                    <Link href="/nutzungsbedingungen" className="underline">{decision.ground}</Link>
                  ) : (
                    decision.ground
                  )}
                </Row>
                <Row label="Begründung">{decision.explanation}</Row>
              </dl>
            </section>

            <section className="mb-4 rounded-xl border border-border p-4">
              <h2 className="mb-2 text-sm font-semibold">Widerspruch</h2>
              {decision.objection ? (
                <div className="space-y-2 text-sm">
                  <p className="text-muted-foreground">
                    Dein Widerspruch vom {new Date(decision.objected_at!).toLocaleString("de-DE")}:
                  </p>
                  <p className="whitespace-pre-wrap rounded bg-muted/50 p-2">{decision.objection}</p>
                  {decision.objection_status === "pending" ? (
                    <p>Unser Team prüft deinen Widerspruch und meldet sich hier bei dir.</p>
                  ) : decision.objection_status === "superseded" ? (
                    <>
                      <p className="font-medium">
                        Unser Team hat den Inhalt inzwischen geprüft; eine neue Entscheidung ersetzt die, gegen die du
                        widersprochen hast.{" "}
                        {decision.superseded_by && (
                          <Link href={`/moderation/decisions/${decision.superseded_by}`} className="underline">
                            Gegen die neue Entscheidung kannst du dort widersprechen.
                          </Link>
                        )}
                      </p>
                      {decision.objection_response && (
                        <p className="whitespace-pre-wrap rounded bg-muted/50 p-2">{decision.objection_response}</p>
                      )}
                    </>
                  ) : (
                    <>
                      <p className="font-medium">
                        {decision.objection_status === "accepted"
                          ? decision.restriction === "removed" && decision.content_purged
                            ? "Wir haben deinem Widerspruch stattgegeben. Der Inhalt war zu diesem Zeitpunkt bereits endgültig gelöscht und konnte nicht wiederhergestellt werden."
                            : "Wir haben deinem Widerspruch stattgegeben."
                          : "Wir haben deinen Widerspruch geprüft und halten an der Entscheidung fest."}
                      </p>
                      {decision.objection_response && (
                        <p className="whitespace-pre-wrap rounded bg-muted/50 p-2">{decision.objection_response}</p>
                      )}
                    </>
                  )}
                </div>
              ) : decision.can_object ? (
                <form onSubmit={(e) => { e.preventDefault(); submitObjection(); }} className="space-y-2">
                  <p className="text-sm text-muted-foreground">
                    Wenn du die Entscheidung für falsch hältst, kannst du innerhalb von sechs Monaten widersprechen.
                    Ein Mitglied unseres Teams prüft den Widerspruch — nicht automatisiert.
                  </p>
                  <textarea
                    value={objection}
                    onChange={(e) => setObjection(e.target.value)}
                    placeholder="Warum ist die Entscheidung aus deiner Sicht falsch?"
                    maxLength={2000}
                    rows={4}
                    className="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                  />
                  <Button type="submit" size="sm" disabled={sending || objection.trim().length < 10}>
                    Widerspruch senden
                  </Button>
                </form>
              ) : decision.lifted_at ? (
                <p className="text-sm text-muted-foreground">
                  Die Entscheidung ist aufgehoben; ein Widerspruch ist nicht mehr nötig.
                </p>
              ) : decision.superseded ? (
                <p className="text-sm text-muted-foreground">
                  Diese Entscheidung wurde durch eine spätere ersetzt.{" "}
                  {decision.superseded_by && (
                    <Link href={`/moderation/decisions/${decision.superseded_by}`} className="underline">
                      Widersprechen kannst du der neuen Entscheidung.
                    </Link>
                  )}
                </p>
              ) : (
                <p className="text-sm text-muted-foreground">Die Frist für einen Widerspruch ist abgelaufen.</p>
              )}
            </section>

            <section className="rounded-xl border border-border p-4 text-sm text-muted-foreground">
              <h2 className="mb-2 font-semibold text-foreground">Weitere Rechtsbehelfe</h2>
              <p>
                Unabhängig von einem Widerspruch kannst du dich an eine zertifizierte außergerichtliche
                Streitbeilegungsstelle nach Art. 21 des Digital Services Act wenden oder den Rechtsweg zu den
                Gerichten beschreiten. Fragen erreichen uns unter{" "}
                <a href="mailto:kontakt@klarsocial.eu" className="underline">kontakt@klarsocial.eu</a>.
              </p>
            </section>
          </>
        )}
      </main>
    </div>
  );
}
