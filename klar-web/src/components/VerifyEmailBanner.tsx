"use client";

import { useState } from "react";
import { MailWarning } from "lucide-react";
import { auth } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";

// Shown on every page (rendered by the root layout, above each page's own
// header) until the signed-in user has verified their email. Unverified
// accounts can't get admin access and have no working password reset, so
// this has to be hard to miss rather than tucked away in settings.
export default function VerifyEmailBanner() {
  const { user } = useAuth();
  const [status, setStatus] = useState<"idle" | "sending" | "sent">("idle");

  // Strictly `false`: the field is only present on GET /users/me and the
  // login/register responses, so undefined means "unknown", not "unverified".
  if (!user || user.email_verified !== false) return null;

  const resend = async () => {
    setStatus("sending");
    try {
      await auth.resendVerification(user.email);
    } catch {
      // The endpoint answers the same way whatever happens; a failure here
      // is a network error, and the button can simply be pressed again.
      setStatus("idle");
      return;
    }
    setStatus("sent");
  };

  return (
    <div className="border-b border-amber-300 bg-amber-50 text-amber-900 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-100">
      <div className="mx-auto flex max-w-2xl flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 text-sm">
        <MailWarning size={16} className="shrink-0" />
        <span className="flex-1">
          Please verify your email address. We sent a link to{" "}
          <strong className="break-all">{user.email}</strong>.
        </span>
        {status === "sent" ? (
          <span className="font-medium">New link sent — check your inbox.</span>
        ) : (
          <button
            type="button"
            onClick={resend}
            disabled={status === "sending"}
            className="font-medium underline underline-offset-4 disabled:opacity-60"
          >
            {status === "sending" ? "Sending…" : "Resend email"}
          </button>
        )}
      </div>
    </div>
  );
}
