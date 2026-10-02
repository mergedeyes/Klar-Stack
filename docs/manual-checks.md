# Manual checks

CI (`ci.yml`) type-checks, lints, and runs the backend and browser tests. This
list covers what those don't: real phones and browsers, real-time updates,
emails arriving, what another account can see, and what happens when the
network fails.

Copy the sections that match what a change touches into the PR's test plan and
tick them off against a local stack or the passcode-gated site. Use two
accounts (in two browsers or a private window) wherever a check says "the other
account". The anonymised production snapshot (`tools/prod-snapshot/`) gives
realistic data volumes for the paging checks.

## Any UI change

- [ ] Works at phone width (about 375 px): no horizontal scroll, nothing hidden behind the top nav or the browser's address bar
- [ ] Works in light and dark mode
- [ ] A short page doesn't scroll and the footer sits on the bottom edge; on a long page the footer stays pinned while scrolling and nothing ends up hidden behind it
- [ ] Loading state appears (skeleton or spinner) and is replaced by content, an empty state or an error, never left spinning
- [ ] Browser console shows no new errors or React warnings

## Paginated lists

Applies to the home feed, discovery feed and profile grid, and to any list that
loads more as you scroll.

- [ ] A list longer than one page keeps loading as you scroll, and stops at the real end
- [ ] No post appears twice and none is skipped at a page boundary (check around posts created in the same second)
- [ ] A list shorter than one page shows everything and never shows a loading spinner at the end
- [ ] On a tall window where the first page doesn't fill the screen, the next page loads without scrolling
- [ ] Switching quickly between two lists of the same kind (e.g. two profiles) never mixes their items
- [ ] With the network cut off (DevTools → Network → Offline) while loading more, you get an error or retry option rather than an endless spinner or a request loop; after reconnecting, retrying continues where it stopped
- [ ] Creating or deleting a post updates the list without breaking further paging

## Privacy and visibility

- [ ] A private account's posts, profile grid and post permalinks are hidden from the other account until it's an accepted follower
- [ ] After unfollowing or being blocked, the other account can no longer open the posts (reload the page; media links stop working once their signature expires)
- [ ] Content hidden by moderation is visible only to its owner, who sees "Hidden — only you can see this post" with a link to Moderation; content removed by moderation is gone for everyone, its owner included
- [ ] Blocking hides each account's posts from the other's discovery feed, and removes pending follow requests both ways
- [ ] Other users' email addresses never appear in any response (check the Network tab on profiles, follower lists, notifications and search)
- [ ] Settings → "Personalised Discovery" is off for a new account, and likes and comments made then never appear in the data download's `discovery_interactions`; after switching it on they do (with `personalization_consented_at` in the profile); switching it off asks first, stays off after a reload, and empties `discovery_interactions`
- [ ] The home feed's order never changes with likes or comments (newest first, whatever the setting)

## Real-time (notifications and chat)

- [ ] A like, comment, follow or follow request from the other account shows up without reloading
- [ ] Chat messages, edits, deletions and reactions appear on the other side live; an edit or deletion doesn't light up the unread badge
- [ ] On a phone, /chats shows the list full width; tapping a chat opens it full screen with the input above the footer, reply/react buttons are visible without hovering, and the back arrow returns to the list
- [ ] The unread badges clear when the conversation or dropdown is opened
- [ ] After the laptop sleeps or the network drops briefly, the stream reconnects and events arrive again

## Accounts and sessions

- [ ] Settings → "Keep my account after the test" is off for a new account; ticking it shows "Requested on …" and stays ticked after a reload; unticking clears it
- [ ] Register, verify the email via the link, log in, log out
- [ ] Before verifying, the banner says posting, commenting, messaging and reporting need a verified address; trying any of them shows that message, liking and following work
- [ ] An unverified account gets a reminder email a week before its deletion (move `users.created_at` back 24 days on a local stack and wait for the hourly sweep, or restart the backend)
- [ ] Session survives an idle period longer than 15 minutes (the access token refreshes silently)
- [ ] Password reset and change password work; changing the password signs other devices out at once (their next click goes to the login page) while this device stays signed in
- [ ] Asking for a reset link twice within five minutes sends one email
- [ ] Data export downloads a ZIP whose `data.json` and images match the account
- [ ] Account deletion asks for the password (a wrong one is refused) and removes the profile, posts and media; the chat partner sees the "account deleted" notice; the like, comment and follower counts on other people's posts and profiles drop accordingly
- [ ] A deleted or mistyped profile shows "Profil nicht gefunden" instead of jumping to the feed
- [ ] Clearing the bio or display name in Edit profile and saving empties it; "Remove photo" removes the picture
- [ ] Locking an account or changing its password closes an open notification stream on its other devices within half a minute (the bell stops updating there)
- [ ] Locking an account in `/admin/security` signs it out within seconds on every device (next request goes to the login page) and sends an email whose link sets a new password; replying to that email goes to kontakt@klarsocial.eu
- [ ] Logging in to a locked account with the right password shows "Your account is locked" (a wrong password shows the usual error); "Send the link again" works once, then asks to wait 15 minutes
- [ ] Setting a new password through the link unlocks the account; the incident log shows "Unlocked by new password" and keeps the assessment
- [ ] An account that posts 10+ comments within a few minutes (or the same text 3+ times) shows up under "Needs review" in `/admin/security`, as does one with a pending spam report; a single post doesn't
- [ ] "Review account" in the report queue opens `/admin/review/<name>` with the reason filled in; the review needs a reason, then shows signals, message counts (no message text), posts, comments, likes, follows, reports and history as expandable sections
- [ ] Deciding "Lock as possibly hacked" locks the account (it lands in the incident log with the note); "Bot or spam-only account" suspends it permanently with a statement that says why; "No action" just closes the review
- [ ] An account with a verified @klarsocial.eu address shows up under Settings → Official accounts (an unverified one doesn't); renaming it to a staff name like "Klar" needs a reason, works, appears in the rename log, and the profile opens at /users/klar; "me" or a taken name is refused
- [ ] The verification email after sign-up has no Reply-To; other emails (password reset, moderation notice) reply to kontakt@klarsocial.eu
- [ ] Emails show the styled layout with a blue button in the Gmail app on a phone, also for a non-Google address added to the app (IMAP, GMX, Ionos), and in Gmail on the web
- [ ] The verification, account-locked and legal-update emails start with "Hallo <username>,"; the legal-update email's footer says it can't be unsubscribed from and why

## Posts and media

- [ ] Upload a photo (portrait and landscape): correct aspect ratio in feed, grid and modal; no location data left in the downloaded file
- [ ] Edit a caption, delete a post: it disappears from the feed, the profile grid and its permalink
- [ ] Counters (likes, comments, posts, followers) stay correct after toggling quickly several times
- [ ] Opening a post from a feed or profile shows /posts/… in the address bar; back, Escape and ✕ close it and restore the address; reloading keeps the post open
- [ ] Share: on a phone it opens the share sheet, on desktop it copies the link ("Link copied")
- [ ] Opening a shared /posts/… link shows the post page (not a modal over the feed): on desktop the card fits the screen with the comment field visible; on a phone the page scrolls and the comment field stays above the footer
- [ ] Pasting a public post's link into WhatsApp/Signal shows a preview with name, caption start and image; a private account's post shows no preview (only after the passcode gate is gone — bots can't pass it)

## Moderation

- [ ] Reporting a post, comment, user or chat message from the other account works and appears in `/admin/reports`, one card per item with all its reports; a second report on the same item by the same account is refused
- [ ] The report dialog shows crisis lines for self-harm, "don't download or share" for CSAM and HateAid for intimate images, and after sending offers to block the author
- [ ] A brand-new or unverified account's CSAM report doesn't hide the post (it tops the queue instead); an account older than a day does hide it; the sixth CSAM/intimate-image report in a day is refused
- [ ] A CSAM, intimate-image or terrorism report sends every admin an alert email with a link and no content; a second report on the same item for the same reason sends none; CSAM and intimate-image cards in the queue show no thumbnail
- [ ] "Dismiss all" closes every report on the item and lifts an automatic hide or warning only when no other report or rights claim holds it; "Dismiss only this report" leaves the others pending
- [ ] Removing makes the post or comment disappear for everyone, its author included; a removed comment with replies shows "Removed by moderation" in its place; an accepted objection brings the content back as it was
- [ ] A post reported as CSAM and deleted by its author still gets a strike when the admin confirms the violation from the evidence copy; a spam report on a post its author deleted closes as "Content no longer available"
- [ ] A reported chat message appears in the queue without its text; its evidence record shows it with the ten messages before it; "Delete message" removes it for both sides; the evidence survives the sender deleting the message or the account
- [ ] An objection to an automatic warning shows on the report card; dismissing asks for an answer and accepts the objection, removing marks it as replaced and the author can object to the removal
- [ ] Settings → Moderation shows each report's outcome ("action was taken against the account", "content no longer available", …); a dismissed report can be sent back once with a note and comes back in the queue as "Re-check requested"
- [ ] "Decide on the account" on an account report opens its standing page: a measure without strikes needs an explanation, which appears in the user's statement (with no score); the report closes as "action was taken against the account"
- [ ] An account's standing page lists its pending reports, ticked, also when opened from the profile or the decision log instead of the queue; a measure then answers them ("Report or notice" in the Decision log, the reports close); with none ticked or none pending it shows "Own initiative", is found by that source filter (not only under "Any source"), and its statement says the team came across the account itself
- [ ] "Remove parts of the profile" removes the picture, bio, display name or username (replaced by user_…, changeable at once); the statement lists what went; an accepted objection puts them back
- [ ] "Moderate" on a post or comment (admins) opens a case; with "An authority's order" the statement names the authority and the reference
- [ ] Only an admin sees the shield button in the top nav (with a red dot while something waits); it opens `/admin`, which lists what's waiting (urgent first) and then every admin page under Moderation, Accounts, Legal and Testing, with badges; Settings no longer lists the admin pages
- [ ] The Decision log filters by decision, reason, source, admin and account and pages with "Load more"
- [ ] `/admin/audit` (Moderation tools → Legal) downloads a ZIP for the chosen period only once a reason is given; its CSVs open in Excel with umlauts and columns intact; accounts appear as K-… codes (the same in every file of one export, different in the next), with no content and no email addresses; "With identities" shows usernames instead and is marked in the list of earlier exports, where every export appears with its reason (and in the next export's `exporte.csv`)
- [ ] A report for a likely-illegal reason immediately creates a record under `/admin/evidence` with the reported state (the report in the queue links to it); a spam report creates nothing
- [ ] Editing the reported item (caption, comment, bio, avatar) adds a version to the timeline; deleting it (as admin, as its author, or with the account) keeps the record and marks it deleted
- [ ] Opening an evidence record or one of its files asks for a reason, and the audit trail shows exactly one entry per action
- [ ] Dismissing the last report on preserved content purges the record at the next sweep; a legal hold stops the purge
- [ ] A removal, automatic hide or warning gives the author a "Klar" notice in the bell and an email linking to the statement; a CSAM statement only arrives after an admin sends it from `/admin/moderation`
- [ ] The author can object once from the statement page; the admin's answer shows up there and in the bell; the reporter sees the outcome under Settings → Moderation
- [ ] "Glorifying Nazism/fascism, extremist symbols" is Grave (60 points): a first case suggests a warning, a second one a permanent suspension
- [ ] Removing a report shows "If removed, classify as" with the report's own reason first and the criterion below; picking a type of another reason, or "No strike", asks for a justification and keeps Remove disabled until it's filled
- [ ] After removal the author's statement names the type, its criterion and the points (not the justification); Settings → Moderation shows the strike and its expiry date
- [ ] A third removal for the same reason within 30 days shows "(repeat, ×1.5)" and the statement explains it
- [ ] In `/admin/standing`, "Show content" on a strike shows the removed text, the post / replied-to comment, the reports and who removed it; the evidence link appears for likely-illegal reasons
- [ ] A "spam" report classified as Holocaust denial (extremist promotion) on removal still gets an evidence record, marked "Report to authorities: recommended"; an attack threat is marked "required", listed first in `/admin/evidence`, and isn't purged until a report to the authorities is recorded
- [ ] The report dialog offers the new reasons (extremism, intimate images, terrorism, scam, illegal goods); an "intimate images" report hides the post at once, a "terrorism" report shows it behind a warning
- [ ] `/admin/standing` lists accounts from 25 points on, with a suggestion: a warning first, a suspension only once the account was warned
- [ ] A suspended account gets a statement and a red notice under Settings → Moderation; posting, commenting, liking, following, chatting and editing the profile fail with a "suspended until" message; reading, objecting, exporting, deleting own posts and deleting the account still work
- [ ] A permanent suspension shows the deletion date under Settings → Moderation (and "deletion waits for the objection" in `/admin/standing` while one is pending); two weeks before, a reminder email arrives
- [ ] While suspended, the profile, posts and comments are gone for another account (profile 404, not in search, feed or discovery); "Lift suspension" or an accepted objection brings everything back

## Feedback

- [ ] Signed in, the footer shows "Feedback"; signed out it doesn't
- [ ] Sending from a page records that page (without its query string); unticking "Include technical details" sends no page, browser or screen size
- [ ] The entry appears in `/admin/feedback`; "Done" hides it from the open list and "Show done" brings it back
- [ ] On a phone (iPhone and Android), "Add screenshot" opens the photo picker; a screenshot taken on the device attaches, previews, can be removed, and shows in `/admin/feedback` (clicking opens it full size)

## Legal pages and passcode gate

- [ ] Impressum, Datenschutz, Nutzungsbedingungen and Transparenz load without the passcode
- [ ] Signed out, "Rechtswidrige Inhalte melden" in the footer opens the notice form (not the passcode gate); a notice about a post, a comment link ("Link" on the comment) or a profile goes into the queue as a public notice; the confirmation email's link shows the status, and the decision arrives by email; a CSAM notice works without name and email
- [ ] If the change adds a sub-processor or changes what data is kept, the Datenschutz and Transparenz pages say so
- [ ] After a deploy with a new file in `klar-web/legal-updates/` (once the page shows its new Stand date), `/admin/legal-updates` lists it and every older account sees a dialog with the summary on its next visit; it can only be accepted, the legal pages and Settings stay reachable, and accepting makes it disappear for good; accounts created afterwards never see it
- [ ] `/admin/legal-updates` filters by a From/To date range (both days included) and sorts by latest/oldest, most/least accepted (share of accounts), most/least emails; Reset restores the default
- [ ] A privacy-only notice has "Verstanden" instead; verified addresses get an email (replies go to kontakt@), unverified ones don't; the admin page counts emails and acceptances

## Rights claims

- [ ] Signed out, "Rechteverletzung melden" in the footer opens the form (not the passcode gate) and a claim can be submitted; the confirmation email arrives with a working status link
- [ ] An evidence request reaches the claimant by email and can be answered on the status page
- [ ] Accepting hides the post; its author gets a statement naming the work, not the claimant; a successful objection makes the post visible again and emails the claimant
