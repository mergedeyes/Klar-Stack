// Dead-man's-switch for the frontend, the same scheme as the backend's
// (Klar/src/uptime.rs) and the backup sidecar's: every few minutes the
// server renders its own /welcome page and pings UPTIME_HEALTHCHECK_URL
// (its own healthchecks.io check), or /fail when the page doesn't render.
// healthchecks.io alerts on a failure ping at once and on missing pings
// after the grace period, which covers the container being down.
//
// /welcome is the passcode page, reachable without the gate. The pings
// carry no body: the privacy policy promises healthchecks.io only
// content-free success or failure signals.

// Matches the check's period on healthchecks.io (5 minutes, grace 10).
const INTERVAL_MS = 5 * 60 * 1000;
// register() must finish before the server accepts requests, so the first
// round waits until it is surely listening.
const FIRST_RUN_MS = 60 * 1000;

export function startUptimePing() {
  const url = process.env.UPTIME_HEALTHCHECK_URL?.trim().replace(/\/+$/, "");
  if (!url) {
    console.warn("UPTIME_HEALTHCHECK_URL not set -- nobody is alerted when the frontend goes down");
    return;
  }
  // The standalone server listens on HOSTNAME (0.0.0.0 if unset).
  const self = `http://${process.env.HOSTNAME || "127.0.0.1"}:${process.env.PORT || "3000"}/welcome`;

  const round = async () => {
    let target = url;
    try {
      const res = await fetch(self, { signal: AbortSignal.timeout(10_000), cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    } catch (err) {
      console.error("Uptime check failed:", err);
      target = `${url}/fail`;
    }
    try {
      const res = await fetch(target, { signal: AbortSignal.timeout(10_000) });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
    } catch (err) {
      console.warn("Uptime ping failed:", err);
    }
  };

  setTimeout(() => {
    void round();
    setInterval(() => void round(), INTERVAL_MS).unref();
  }, FIRST_RUN_MS).unref();
}
