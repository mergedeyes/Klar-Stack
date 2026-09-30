"use client";

import { useEffect, useRef, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { AlertTriangle, Lock, LockOpen } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminEvidenceApi, type EvidenceDetail, type EvidenceEvent, type EvidenceFile, type EvidenceVersion } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import EvidenceStatus, { AuthorityReportBadge } from "@/components/EvidenceStatus";
import { REASON_LABELS, TRIGGER_LABELS } from "@/lib/moderation";

// The snapshot's shape, per target type (see evidence.rs on the backend).
interface Person {
  id: string;
  username: string;
  display_name: string | null;
  email: string;
  created_at: string;
}
interface Snapshot {
  post?: { caption: string | null; created_at: string; edited_at: string | null };
  comment?: { body: string; created_at: string; edited_at: string | null };
  profile?: { username: string; display_name: string | null; bio: string | null; created_at: string };
  context?: {
    post: { id: string; caption: string | null; author_id: string } | null;
    parent_comment: { id: string; body: string; author_id: string } | null;
  };
  author?: Person | null;
}

const CAUSE_LABELS: Record<EvidenceVersion["cause"], string> = {
  reported: "As reported",
  edited: "After an edit",
  deleted: "At deletion",
};

const CAUSE_TEXT: Record<string, string> = {
  reported: "when reported",
  edited: "after an edit",
  deleted: "at deletion",
};

const ACTION_LABELS: Record<string, string> = {
  created: "Record opened",
  version_added: "Version captured",
  content_deleted: "Original deleted",
  decided: "Decided",
  viewed: "Opened",
  file_viewed: "Viewed file",
  hold_set: "Legal hold set",
  hold_lifted: "Legal hold lifted",
  authority_report: "Reported to authority",
  purged: "Purged",
};

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[7rem_1fr] gap-2 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words">{children}</span>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-4 rounded-xl border border-border p-3">
      <h2 className="mb-2 text-sm font-semibold">{title}</h2>
      <div className="space-y-1.5">{children}</div>
    </section>
  );
}

export default function EvidenceDetailPage() {
  const { id } = useParams<{ id: string }>();
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [reason, setReason] = useState("");
  const [detail, setDetail] = useState<EvidenceDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Object URLs of the files shown so far, keyed by file id.
  const [fileUrls, setFileUrls] = useState<Record<string, string>>({});
  const fileUrlsRef = useRef(fileUrls);
  useEffect(() => { fileUrlsRef.current = fileUrls; }, [fileUrls]);
  useEffect(() => () => Object.values(fileUrlsRef.current).forEach(URL.revokeObjectURL), []);

  const [holdReason, setHoldReason] = useState("");
  const [authority, setAuthority] = useState("");
  const [reportedOn, setReportedOn] = useState(() => new Date().toLocaleDateString("sv-SE"));
  const [reference, setReference] = useState("");
  const [authorityNote, setAuthorityNote] = useState("");

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong");
    } finally {
      setBusy(false);
    }
  };

  const open = () => run(async () => setDetail(await adminEvidenceApi.open(id, reason)));

  // The backend logged the action; mirror it into the trail on screen.
  // Re-opening the record to refresh it would log an extra "opened" event
  // for every action and pad the audit trail.
  const appendEvent = (action: string, eventReason: string | null, details: EvidenceEvent["details"] = null) =>
    setDetail((prev) => prev && {
      ...prev,
      events: [...prev.events, {
        id: crypto.randomUUID(),
        actor_id: user?.id ?? null,
        actor_username: user?.username ?? null,
        action,
        reason: eventReason,
        details,
        created_at: new Date().toISOString(),
      }],
    });

  const showFile = (file: EvidenceFile) =>
    run(async () => {
      const url = await adminEvidenceApi.fileUrl(id, file.id, reason);
      setFileUrls((prev) => ({ ...prev, [file.id]: url }));
      appendEvent("file_viewed", reason.trim(), { file_id: file.id });
    });

  const hideFile = (fileId: string) => {
    setFileUrls((prev) => {
      const { [fileId]: url, ...rest } = prev;
      if (url) URL.revokeObjectURL(url);
      return rest;
    });
  };

  const toggleHold = () =>
    run(async () => {
      if (!detail) return;
      const summary = await adminEvidenceApi.setHold(id, !detail.legal_hold, holdReason);
      setDetail((prev) => prev && { ...prev, ...summary });
      appendEvent(summary.legal_hold ? "hold_set" : "hold_lifted", holdReason.trim());
      setHoldReason("");
    });

  const recordAuthorityReport = () =>
    run(async () => {
      await adminEvidenceApi.recordAuthorityReport(id, authority, reportedOn, reference, authorityNote);
      appendEvent("authority_report", authorityNote.trim() || null, {
        authority: authority.trim(),
        reported_on: reportedOn,
        reference: reference.trim() || null,
      });
      setAuthority("");
      setReference("");
      setAuthorityNote("");
    });

  if (authLoading || !user) return null;

  const firstAuthor = (detail?.versions[0]?.content as Snapshot | undefined)?.author ?? null;
  const isCsam = detail?.reasons.includes("csam") ?? false;

  return (
    <div className="flex-1 bg-background pb-12">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Evidence record</span>
        {detail && <EvidenceStatus record={detail} />}
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {detail?.authority_report && (
          <div className="mb-3 flex">
            <AuthorityReportBadge record={detail} />
          </div>
        )}
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {!detail ? (
          <form
            onSubmit={(e) => { e.preventDefault(); open(); }}
            className="rounded-xl border border-border p-4"
          >
            <label htmlFor="reason" className="mb-1 block text-sm font-semibold">
              Why are you opening this record?
            </label>
            <p className="mb-2 text-sm text-muted-foreground">
              Logged with your account and the time, along with every file you view.
            </p>
            <textarea
              id="reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="e.g. reviewing report before deciding; preparing report to BKA"
              maxLength={1000}
              rows={2}
              className={`${inputClass} mb-3`}
            />
            <Button type="submit" size="sm" disabled={busy || !reason.trim()}>Open</Button>
          </form>
        ) : (
          <>
            {isCsam && (
              <div className="mb-4 flex gap-2 rounded-md bg-destructive/10 p-3 text-sm text-destructive">
                <AlertTriangle size={16} className="mt-0.5 shrink-0" />
                <p>
                  Reported as child sexual abuse material. View images only as far as needed to decide and
                  to report; never download, copy or share them.
                </p>
              </div>
            )}

            <Section title="Record">
              <Field label="Type"><span className="capitalize">{detail.target_type}</span></Field>
              <Field label="Reasons">{detail.reasons.map((r) => REASON_LABELS[r] ?? r).join(", ")}</Field>
              <Field label="Opened">{new Date(detail.created_at).toLocaleString()}</Field>
              <Field label="Original">
                {detail.content_deleted_at && detail.deletion_trigger
                  ? `${TRIGGER_LABELS[detail.deletion_trigger]} on ${new Date(detail.content_deleted_at).toLocaleString()}`
                  : "Still online"}
              </Field>
              {detail.decided_at && (
                <Field label="Decision">
                  {detail.decision} on {new Date(detail.decided_at).toLocaleString()}
                  {detail.decision_note && <> — &ldquo;{detail.decision_note}&rdquo;</>}
                </Field>
              )}
              {detail.retain_until && !detail.purged_at && (
                <Field label="Kept until">
                  {new Date(detail.retain_until).toLocaleDateString()}
                  {detail.legal_hold && " (on hold: kept until the hold is lifted)"}
                </Field>
              )}
              {detail.purged_at && <Field label="Purged">{new Date(detail.purged_at).toLocaleString()}</Field>}
            </Section>

            {detail.versions.map((version, index) => {
              const snap = version.content as Snapshot;
              return (
                <Section
                  key={version.id}
                  title={`${index + 1}. ${CAUSE_LABELS[version.cause]} · ${new Date(version.captured_at).toLocaleString()}`}
                >
                  {snap.post && (
                    <>
                      <Field label="Caption"><span className="whitespace-pre-wrap">{snap.post.caption ?? "—"}</span></Field>
                      <Field label="Posted">{new Date(snap.post.created_at).toLocaleString()}</Field>
                      {snap.post.edited_at && <Field label="Edited">{new Date(snap.post.edited_at).toLocaleString()}</Field>}
                    </>
                  )}
                  {snap.comment && (
                    <>
                      <Field label="Comment"><span className="whitespace-pre-wrap">{snap.comment.body}</span></Field>
                      <Field label="Posted">{new Date(snap.comment.created_at).toLocaleString()}</Field>
                      {snap.comment.edited_at && <Field label="Edited">{new Date(snap.comment.edited_at).toLocaleString()}</Field>}
                      {snap.context?.parent_comment && (
                        <Field label="In reply to">
                          <span className="whitespace-pre-wrap text-muted-foreground">{snap.context.parent_comment.body}</span>
                        </Field>
                      )}
                      {snap.context?.post && (
                        <Field label="On post">
                          <span className="whitespace-pre-wrap text-muted-foreground">{snap.context.post.caption ?? "(no caption)"}</span>
                        </Field>
                      )}
                    </>
                  )}
                  {snap.profile && (
                    <>
                      <Field label="Username">{snap.profile.username}</Field>
                      <Field label="Display name">{snap.profile.display_name ?? "—"}</Field>
                      <Field label="Bio"><span className="whitespace-pre-wrap">{snap.profile.bio ?? "—"}</span></Field>
                    </>
                  )}
                  {version.files.map((file) => (
                    <div key={file.id} className="rounded-md bg-muted/40 p-2">
                      <div className="mb-1 flex items-center justify-between gap-2 text-sm">
                        <span>{file.kind === "avatar" ? "Avatar" : "Image"} · {file.content_type}</span>
                        {fileUrls[file.id] ? (
                          <Button size="sm" variant="outline" onClick={() => hideFile(file.id)}>Hide</Button>
                        ) : (
                          <Button size="sm" variant="outline" onClick={() => showFile(file)} disabled={busy}>Show</Button>
                        )}
                      </div>
                      <p className="break-all text-xs text-muted-foreground">
                        {file.copied_at
                          ? `SHA-256 ${file.sha256} · ${file.size_bytes} bytes`
                          : "Copy into the evidence zone still pending — served from the original"}
                      </p>
                      {fileUrls[file.id] && (
                        // A blob: URL for a file served with no-store; next/image can't load those.
                        // eslint-disable-next-line @next/next/no-img-element
                        <img src={fileUrls[file.id]} alt="Preserved file" className="mt-2 max-h-96 rounded" />
                      )}
                    </div>
                  ))}
                </Section>
              );
            })}

            {firstAuthor && (
              <Section title="Author when reported">
                <Field label="Username">{firstAuthor.username}</Field>
                <Field label="Display name">{firstAuthor.display_name ?? "—"}</Field>
                <Field label="Email">{firstAuthor.email}</Field>
                <Field label="User ID"><code className="text-xs">{firstAuthor.id}</code></Field>
                <Field label="Registered">{new Date(firstAuthor.created_at).toLocaleString()}</Field>
              </Section>
            )}

            {detail.reports.length > 0 && (
              <Section title="Reports">
                {detail.reports.map((r) => (
                  <div key={r.id} className="text-sm">
                    <span className="font-medium">{REASON_LABELS[r.reason] ?? r.reason}</span>
                    <span className="text-muted-foreground"> · {new Date(r.created_at).toLocaleString()} · {r.status}</span>
                    {r.details && <p className="italic text-muted-foreground">&ldquo;{r.details}&rdquo;</p>}
                  </div>
                ))}
              </Section>
            )}

            {!detail.purged_at && (
              <Section title={detail.legal_hold ? "Lift legal hold" : "Legal hold"}>
                <p className="text-sm text-muted-foreground">
                  {detail.legal_hold
                    ? "Lifting the hold lets the record be purged once its retention period has ended."
                    : "Keeps the record past its retention period, e.g. for an authority request or proceedings."}
                </p>
                <input
                  value={holdReason}
                  onChange={(e) => setHoldReason(e.target.value)}
                  placeholder="Reason (e.g. request from Staatsanwaltschaft, Az. …)"
                  maxLength={1000}
                  className={inputClass}
                />
                <Button
                  size="sm"
                  variant={detail.legal_hold ? "outline" : "destructive"}
                  onClick={toggleHold}
                  disabled={busy || !holdReason.trim()}
                >
                  {detail.legal_hold
                    ? <><LockOpen size={14} className="mr-1" /> Lift hold</>
                    : <><Lock size={14} className="mr-1" /> Set hold</>}
                </Button>
              </Section>
            )}

            <Section title="Record a report to an authority">
              <p className="text-sm text-muted-foreground">
                The report itself happens outside Klar; this records that, when and to whom it was made.
              </p>
              <input value={authority} onChange={(e) => setAuthority(e.target.value)} placeholder="Authority (e.g. BKA, jugendschutz.net)" maxLength={1000} className={inputClass} />
              <input type="date" value={reportedOn} onChange={(e) => setReportedOn(e.target.value)} className={inputClass} />
              <input value={reference} onChange={(e) => setReference(e.target.value)} placeholder="Their reference / case number (optional)" maxLength={1000} className={inputClass} />
              <input value={authorityNote} onChange={(e) => setAuthorityNote(e.target.value)} placeholder="Note (optional)" maxLength={1000} className={inputClass} />
              <Button size="sm" onClick={recordAuthorityReport} disabled={busy || !authority.trim() || !reportedOn}>
                Record report
              </Button>
            </Section>

            <Section title="Audit trail">
              {detail.events.map((ev) => (
                <div key={ev.id} className="text-sm">
                  <span className="font-medium">{ACTION_LABELS[ev.action] ?? ev.action}</span>
                  <span className="text-muted-foreground">
                    {" "}· {new Date(ev.created_at).toLocaleString()} · {ev.actor_id ? (ev.actor_username ?? `deleted account ${ev.actor_id}`) : "system"}
                  </span>
                  {ev.reason && (
                    <p className="text-muted-foreground">
                      {ev.action === "created" || ev.action === "version_added"
                        ? CAUSE_TEXT[ev.reason] ?? ev.reason
                        : ev.action === "content_deleted"
                          ? TRIGGER_LABELS[ev.reason as keyof typeof TRIGGER_LABELS] ?? ev.reason
                          : <>&ldquo;{ev.reason}&rdquo;</>}
                    </p>
                  )}
                  {ev.details && ev.action === "authority_report" && (
                    <p className="text-muted-foreground">
                      {String(ev.details.authority)} on {String(ev.details.reported_on)}
                      {ev.details.reference ? ` · ref. ${String(ev.details.reference)}` : ""}
                    </p>
                  )}
                </div>
              ))}
            </Section>
          </>
        )}
      </main>
    </div>
  );
}
