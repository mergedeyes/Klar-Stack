# klar-web

The Next.js frontend of Klar. Setup, the checks CI runs and how to start the
whole stack are in the [main README](../README.md); in short, from here:

```sh
npm install
npm run dev     # MailHog, the backend on :3000 and this frontend on :3001
```

- `src/app/`: the pages; `src/components/`: shared components.
- `src/lib/api.ts`: the client for the backend's API.
- `src/proxy.ts`: the passcode gate (it doesn't protect the API).
- `messages/en.json`, `messages/de.json`: the texts in English and German
  (next-intl); `tsc` fails when German lacks a message.
- `e2e/`: the browser tests, see [e2e/README.md](e2e/README.md).
- `legal-updates/`: notices about changed Terms or privacy policy.
