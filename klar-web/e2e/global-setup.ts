import { request } from "@playwright/test";
import { ADMIN_EMAIL, ADMIN_PASSWORD, ADMIN_USERNAME, API, clientIp, withDb } from "./helpers";

// The admin account for the moderation tests. An admin is an address in the
// backend's ADMIN_EMAILS with a verified email; the backend has no way to
// verify an address without its inbox, so this marks it verified straight in
// the throwaway test database (E2E_DATABASE_URL). Without that URL, the
// admin tests skip themselves (see adminSession); CI always sets it.
export default async function globalSetup() {
  if (!process.env.E2E_DATABASE_URL) {
    console.warn("E2E_DATABASE_URL isn't set: the admin tests will be skipped.");
    return;
  }
  const context = await request.newContext();
  try {
    // 409 or 400 on a second run against the same database: already there.
    await context.post(`${API}/auth/register`, {
      headers: { "X-Forwarded-For": clientIp() },
      data: { username: ADMIN_USERNAME, email: ADMIN_EMAIL, password: ADMIN_PASSWORD, accept_terms: true },
    });
  } finally {
    await context.dispose();
  }
  await withDb(async (db) => {
    const res = await db.query("UPDATE users SET email_verified = TRUE WHERE LOWER(email) = $1", [ADMIN_EMAIL]);
    if (res.rowCount !== 1) throw new Error(`Admin account ${ADMIN_EMAIL} wasn't created`);
  });
}
