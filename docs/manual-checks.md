# Manual checks

CI (`lint.yml`) type-checks, lints and runs the backend tests, but it never
clicks through the app. This list covers the behaviour only a person in a
browser can confirm: scrolling, real-time updates, what another account can
see, and what happens when the network fails.

Copy the sections that match what a change touches into the PR's test plan and
tick them off against a local stack or the passcode-gated site. Use two
accounts (in two browsers or a private window) wherever a check says "the other
account". The anonymised production snapshot (`tools/prod-snapshot/`) gives
realistic data volumes for the paging checks.

## Any UI change

- [ ] Works at phone width (about 375 px): no horizontal scroll, nothing hidden behind the top nav or the browser's address bar
- [ ] Works in light and dark mode
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

- [ ] Register, verify the email via the link, log in, log out
- [ ] Session survives an idle period longer than 15 minutes (the access token refreshes silently)
- [ ] Password reset and change password work; changing the password logs out other sessions
- [ ] Data export downloads a ZIP whose `data.json` and images match the account
- [ ] Account deletion removes the profile, posts and media; the chat partner sees the "account deleted" notice

## Posts and media

- [ ] Upload a photo (portrait and landscape): correct aspect ratio in feed, grid and modal; no location data left in the downloaded file
- [ ] Edit a caption, delete a post: it disappears from the feed, the profile grid and its permalink
- [ ] Counters (likes, comments, posts, followers) stay correct after toggling quickly several times

## Moderation

- [ ] Reporting a post, comment or user from the other account works and appears in `/admin/reports`
- [ ] Dismissing restores visibility; removing deletes the content and its media
- [ ] Deleting content (as admin, as its author, or by deleting the account) while it has a pending report for a likely-illegal reason creates a record under `/admin/evidence`, and the report in the queue links to it; with only a spam report, nothing is kept
- [ ] Opening an evidence record or one of its files asks for a reason, and the audit trail shows exactly one entry per action
- [ ] Dismissing the last report on preserved content purges the record at the next sweep; a legal hold stops the purge

## Legal pages and passcode gate

- [ ] Impressum, Datenschutz, Nutzungsbedingungen and Transparenz load without the passcode
- [ ] If the change adds a sub-processor or changes what data is kept, the Datenschutz and Transparenz pages say so
