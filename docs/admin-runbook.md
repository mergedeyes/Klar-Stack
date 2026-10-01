# Admin runbook

How the moderation team handles what reaches it. The admin pages are under
Settings (badges show what is waiting); everything here is decided by a
person, and every decision is logged with who, when and why (Settings →
Decision log).

## What tells you something is waiting

- **Urgent alerts** — an email to every admin address as soon as something
  is reported for child sexual abuse material, intimate images shared
  without consent or terrorism, and when a removal is classified as a
  violation that must be reported to the authorities (DSA Art. 18). The
  email names no content and no person; open the link.
- **Daily digest** — from 06:00 UTC, one email with the counts of pending
  reports (urgent and overdue ones separately), objections, rights claims,
  held-back statements and overdue evidence. Nothing waiting, no email.
- **Badges** in Settings on Reports, Statements & objections, Rights claims
  and Evidence; red when something urgent is among them.

## The report queue (`/admin/reports`)

One card per reported item, with every report on it. Overdue items (30+
days) come first, then critical (CSAM, intimate images, authority orders),
then high (shown behind a warning), then the rest.

- **Dismiss all** closes every report on the item as "no violation"; the
  reporters are told. Automatic hides and warnings that rested only on these
  reports are lifted; restrictions with another basis (a rights claim,
  another pending report) stay. Use "Dismiss only this report" when one
  report on the item is unfounded but the others need a decision.
- **Remove content** decides every report on the item at once: one
  decision, one strike, every reporter told. Pick the violation type from
  the catalog; another reason than the reports', or "No strike", needs a
  justification (internal). The content disappears for everyone, its author
  included, and is deleted for good after the 183-day objection window — an
  accepted objection restores it as it was. CSAM is deleted as soon as its
  evidence copy exists.
- **Deleted by its author**: the card says so and links the preserved copy.
  "Confirm violation" decides on that copy, so deleting doesn't escape the
  strike.
- **Direct messages** are never shown in the queue. Open the evidence copy
  (the message and the ten before it); reading it is logged. "Delete
  message" deletes it for both sides.
- **Accounts**: "Decide on the account" opens its standing page with the
  reports attached; a measure or a removal from the profile there answers
  them. "Review account" opens the activity review; a lock or bot ban there
  answers a report on the account itself, a report on its content stays in
  the queue.
- **Objections** to an automatic hide or warning show on the card: dismissing
  accepts them with the answer you write (shown to the author), removing
  replaces the restriction (the author can object to the removal).
- **Re-checks**: a reporter can ask once to have a dismissal checked again;
  the report comes back marked "Re-check requested" with their note.

## Child sexual abuse material

1. Don't download, screenshot or forward anything. Open the evidence copy
   only as far as needed to decide.
2. Remove it with the CSAM type. The statement to the author is held back
   automatically, so a suspect isn't warned.
3. Report it to the authorities (BKA / jugendschutz.net) outside Klar, then
   record when and to whom in the evidence record ("Report to authority").
   ⚖️ Confirm the process with a lawyer before launch (§ 184b StGB).
4. Then send the held-back statement under Statements & objections. Held
   statements older than a week are marked; a ban's account deletion waits
   for its statement.

## Orders from authorities, and the team's own cases

For content nobody reported: "Open a case" in the queue, or "Moderate" in the
post, comment or profile menu (admins only). Choose "An authority's order"
and enter the authority and its reference, or "The team came across it". The
case lands in the queue and is decided there; the statement names the
source (DSA Art. 17(3)(b)).

**A removal order for terrorist content must be carried out within one hour
of receipt** (Regulation 2021/784, Art. 3(3)):

1. Open the case from the content's menu with the authority and the order's
   reference.
2. Remove it from the queue right away (terrorist propaganda or threat type).
3. Record the order and the time of removal in the evidence record. ⚖️ The
   regulation also requires informing the authority; confirm the channel.

## Accounts

On the standing page (`/admin/standing/<name>`):

- **A measure** (warning, 7 or 30 days, permanent). The suggestion follows
  the score and warns before suspending. Without active strikes, or beyond
  the suggestion, write the explanation the user will read: the score
  doesn't explain it then.
- **Remove parts of the profile**: picture, bio, display name, or the
  username (replaced by `user_…`, which its owner can change at once).
  Classified and objectable like a content removal; an accepted objection
  puts the parts back.
- **Lift suspension** ends it early.

## Retention (retention.rs, evidence.rs, standing.rs)

| What | Deleted |
|---|---|
| Removed posts, comments, profile pictures | 183 days after removal (CSAM: once its evidence copy exists) |
| Reports | 183 days after the decision, unless a strike, evidence or objection rests on them |
| Decisions | 3 years after the decision, its lifting or the objection's answer, unless a strike or suspension rests on them |
| Evidence | dismissed: next sweep; removed: 183 days after the decision; never while on legal hold or before a required authority report is recorded |
| Strikes | 90 days to 1 year by severity; severe ones don't expire |
| Accounts never verified | 30 days after sign-up, a week after a reminder |
| Banned accounts | 183 days after the ban, two weeks after a reminder, not while an objection is pending or the statement is held back |
| Notifications | read: 90 days; unread: 1 year |
