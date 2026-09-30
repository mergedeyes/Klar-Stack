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
- [ ] Content hidden by moderation is visible only to its owner
- [ ] Other users' email addresses never appear in any response (check the Network tab on profiles, follower lists, notifications and search)

## Real-time (notifications and chat)

- [ ] A like, comment, follow or follow request from the other account shows up without reloading
- [ ] Chat messages, edits, deletions and reactions appear on the other side live
- [ ] The unread badges clear when the conversation or dropdown is opened
- [ ] After the laptop sleeps or the network drops briefly, the stream reconnects and events arrive again

## Accounts and sessions

- [ ] Settings → "Keep my account after the test" is off for a new account; ticking it shows "Requested on …" and stays ticked after a reload; unticking clears it
- [ ] Register, verify the email via the link, log in, log out
- [ ] Session survives an idle period longer than 15 minutes (the access token refreshes silently)
- [ ] Password reset and change password work; changing the password logs out other sessions
- [ ] Data export downloads a ZIP whose `data.json` and images match the account
- [ ] Account deletion removes the profile, posts and media; the chat partner sees the "account deleted" notice

## Posts and media

- [ ] Upload a photo (portrait and landscape): correct aspect ratio in feed, grid and modal; no location data left in the downloaded file
- [ ] Edit a caption, delete a post: it disappears from the feed, the profile grid and its permalink
- [ ] Counters (likes, comments, posts, followers) stay correct after toggling quickly several times
- [ ] Opening a post from a feed or profile shows /posts/… in the address bar; back, Escape and ✕ close it and restore the address; reloading keeps the post open
- [ ] Share: on a phone it opens the share sheet, on desktop it copies the link ("Link copied")
- [ ] Opening a shared /posts/… link shows the post page (not a modal over the feed): on desktop the card fits the screen with the comment field visible; on a phone the page scrolls and the comment field stays above the footer
- [ ] Pasting a public post's link into WhatsApp/Signal shows a preview with name, caption start and image; a private account's post shows no preview (only after the passcode gate is gone — bots can't pass it)

## Moderation

- [ ] Reporting a post, comment or user from the other account works and appears in `/admin/reports`
- [ ] Dismissing restores visibility; removing deletes the content and its media
- [ ] A report for a likely-illegal reason immediately creates a record under `/admin/evidence` with the reported state (the report in the queue links to it); a spam report creates nothing
- [ ] Editing the reported item (caption, comment, bio, avatar) adds a version to the timeline; deleting it (as admin, as its author, or with the account) keeps the record and marks it deleted
- [ ] Opening an evidence record or one of its files asks for a reason, and the audit trail shows exactly one entry per action
- [ ] Dismissing the last report on preserved content purges the record at the next sweep; a legal hold stops the purge
- [ ] A removal, automatic hide or warning gives the author a "Klar" notice in the bell and an email linking to the statement; a CSAM statement only arrives after an admin sends it from `/admin/moderation`
- [ ] The author can object once from the statement page; the admin's answer shows up there and in the bell; the reporter sees the outcome under Settings → Moderation

## Feedback

- [ ] Signed in, the footer shows "Feedback"; signed out it doesn't
- [ ] Sending from a page records that page (without its query string); unticking "Include technical details" sends no page, browser or screen size
- [ ] The entry appears in `/admin/feedback`; "Done" hides it from the open list and "Show done" brings it back

## Legal pages and passcode gate

- [ ] Impressum, Datenschutz, Nutzungsbedingungen and Transparenz load without the passcode
- [ ] If the change adds a sub-processor or changes what data is kept, the Datenschutz and Transparenz pages say so

## Rights claims

- [ ] Signed out, "Rechteverletzung melden" in the footer opens the form (not the passcode gate) and a claim can be submitted; the confirmation email arrives with a working status link
- [ ] An evidence request reaches the claimant by email and can be answered on the status page
- [ ] Accepting hides the post; its author gets a statement naming the work, not the claimant; a successful objection makes the post visible again and emails the claimant
