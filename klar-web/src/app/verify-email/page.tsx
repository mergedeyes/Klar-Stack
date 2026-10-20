"use client";

import { useEffect, useRef, useState, Suspense } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { CheckCircle, XCircle, Loader2 } from "lucide-react";
import { auth } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

type State = "loading" | "success" | "expired" | "invalid";

function Verifying() {
  const t = useTranslations("auth.verify");
  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm text-center">
        <CardContent className="pt-8 pb-6">
          <Loader2 size={40} className="mx-auto mb-4 animate-spin text-muted-foreground" />
          <p className="text-sm text-muted-foreground">{t("verifying")}</p>
        </CardContent>
      </Card>
    </main>
  );
}

// 1. Die eigentliche Logik wandert in diese innere Komponente
function VerifyEmailContent() {
  const searchParams = useSearchParams();
  const token = searchParams.get("token");
  const t = useTranslations("auth");
  const [requestState, setRequestState] = useState<State>("loading");
  // Without a token there is nothing to verify; derived instead of set
  // from the effect.
  const state: State = token ? requestState : "invalid";
  const { refreshUser } = useAuth();
  // The link works once. In development React runs every effect twice
  // (StrictMode), and the second request would find the token used and show
  // "Link expired" over the success; the ref keeps it to one request per
  // token. It survives that second run, unlike state set from the first.
  const sentFor = useRef<string | null>(null);

  useEffect(() => {
    if (!token || sentFor.current === token) return;
    sentFor.current = token;

    auth.verifyEmail(token)
      .then(() => {
        setRequestState("success");
        // Re-fetch the signed-in user (if any) so the verify-email banner
        // disappears right away instead of on the next reload.
        refreshUser();
      })
      .catch((err: Error) => {
        const msg = err.message.toLowerCase();
        if (msg.includes("expired") || msg.includes("used") || msg.includes("invalid")) {
          setRequestState("expired");
        } else {
          setRequestState("invalid");
        }
      });
  }, [token, refreshUser]);

  if (state === "loading") {
    return <Verifying />;
  }

  if (state === "success") {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <CheckCircle size={40} className="mx-auto mb-2 text-green-500" />
            <CardTitle className="text-2xl">{t("verify.doneTitle")}</CardTitle>
            <CardDescription>{t("verify.doneText")}</CardDescription>
          </CardHeader>
          <CardFooter className="justify-center">
            <Button asChild>
              <Link href="/feed">{t("verify.goToKlar")}</Link>
            </Button>
          </CardFooter>
        </Card>
      </main>
    );
  }

  const expired = state === "expired";
  // expired, or invalid / no token
  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm text-center">
        <CardHeader>
          <XCircle size={40} className="mx-auto mb-2 text-destructive" />
          <CardTitle className="text-2xl">{expired ? t("verify.expiredTitle") : t("verify.invalidTitle")}</CardTitle>
          <CardDescription>{expired ? t("verify.expiredText") : t("verify.invalidText")}</CardDescription>
        </CardHeader>
        <CardFooter className="flex-col gap-2">
          <Button asChild className="w-full">
            <Link href="/resend-verification">{t("verify.resend")}</Link>
          </Button>
          <Button asChild variant="ghost" className="w-full">
            <Link href="/login">{t("backToSignIn")}</Link>
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}

// 2. Deine exportierte Hauptseite nutzt jetzt Suspense
export default function VerifyEmailPage() {
  return (
    // Als Fallback nutzen wir genau das gleiche Loading-UI wie oben!
    <Suspense fallback={<Verifying />}>
      <VerifyEmailContent />
    </Suspense>
  );
}
