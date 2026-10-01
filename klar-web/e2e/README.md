# Browser tests

Playwright tests for what only a real page shows: the footer layout, the post
modal's address and back button, sharing, the post page on phone and desktop,
the link-preview tags, signing up, verifying, resetting and changing the
password, the data download and account deletion, follow requests, live
notifications and chat messages, the profile grid's paging, and the
moderation and admin flows (reporting posts, comments and profiles, CSAM and
intimate-image hides with held-back statements, classifying removals,
standing, suspensions, objections, evidence with legal holds and authority
reports, rights claims from the public form, official accounts, account
locks and reviews). Each test creates its own users and posts through the
API, so they can run in parallel against any database; lists shared between
tests (the report queue, the incident log) are always searched for the
test's own unique text.

CI runs them on every pull request (the e2e job in `.github/workflows/ci.yml`) against a
fresh stack. Locally, with the backend and frontend running:

```sh
cd klar-web
npx playwright install chromium        # once
E2E_BASE_URL=http://localhost:3001 E2E_API_URL=http://127.0.0.1:3000 npm run test:e2e
```

The defaults are those two addresses (`npm run dev`). The backend should allow
many sign-ups and requests (`AUTH_RATE_LIMIT_PER_MIN=1000`,
`GENERAL_RATE_LIMIT_PER_MIN=10000`) and have no passcode gate configured.

Pass the backend's database as `E2E_DATABASE_URL`: posting, commenting,
messaging and reporting need a verified email address, and `signUp` marks
each test account verified directly in the database, since the link only
goes to an inbox nobody reads. The admin tests also need an admin: start the
backend with `ADMIN_EMAILS=e2e-admin@example.test`, and the global setup
(`global-setup.ts`) registers that account and verifies it. A few tests read
from the database what the app only emails (a reset link) or move timestamps
(`trustedSignUp` makes an account a day old, so its reports hide content).
Use a throwaway database for this, never a real one. Without
`E2E_DATABASE_URL` every test that needs a verified account skips itself. `--project desktop` or `--project phone` runs one screen size;
`--ui` opens Playwright's test browser.
