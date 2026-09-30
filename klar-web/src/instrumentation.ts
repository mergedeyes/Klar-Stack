// Runs once when the Next.js server starts. The uptime ping needs Node's
// timers and a long-lived process, so it only starts in the Node runtime.
export async function register() {
  if (process.env.NEXT_RUNTIME === "nodejs") {
    const { startUptimePing } = await import("./lib/server/uptime");
    startUptimePing();
  }
}
