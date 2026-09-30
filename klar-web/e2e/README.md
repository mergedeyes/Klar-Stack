# Browser tests

Playwright tests for what only a real page shows: the footer layout, the post
modal's address and back button, sharing, the post page on phone and desktop,
the link-preview tags, and the moderation and admin flows (reporting,
classifying removals, standing, suspensions, objections, evidence, account
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

The admin tests need an admin: start the backend with
`ADMIN_EMAILS=e2e-admin@example.test` and pass the backend's database as
`E2E_DATABASE_URL`. The global setup (`global-setup.ts`) registers that
account and marks its email verified directly in the database, and a few
tests read from it what the app only emails (a reset link) or move
timestamps. Use a throwaway database for this, never a real one. Without
`E2E_DATABASE_URL` the admin tests skip themselves. `--project desktop` or `--project phone` runs one screen size;
`--ui` opens Playwright's test browser.
