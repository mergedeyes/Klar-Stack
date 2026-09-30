# Browser tests

Playwright tests for what only a real page shows: the footer layout, the post
modal's address and back button, sharing, the post page on phone and desktop,
and the link-preview tags. Each test creates its own users and posts through
the API, so they can run in parallel against any database.

CI runs them on every pull request (the e2e job in `.github/workflows/ci.yml`) against a
fresh stack. Locally, with the backend and frontend running:

```sh
cd klar-web
npx playwright install chromium        # once
E2E_BASE_URL=http://localhost:3001 E2E_API_URL=http://127.0.0.1:3000 npm run test:e2e
```

The defaults are those two addresses (`npm run dev`). The backend should allow
many sign-ups (`AUTH_RATE_LIMIT_PER_MIN=1000`) and have no passcode gate
configured. `--project desktop` or `--project phone` runs one screen size;
`--ui` opens Playwright's test browser.
