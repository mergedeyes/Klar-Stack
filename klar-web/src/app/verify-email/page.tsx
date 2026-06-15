"use client";

import { useEffect, useState, Suspense } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { CheckCircle, XCircle, Loader2 } from "lucide-react";
import { auth } from "@/lib/api";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

type State = "loading" | "success" | "expired" | "invalid";

// 1. Die eigentliche Logik wandert in diese innere Komponente
function VerifyEmailContent() {
  const searchParams = useSearchParams();
  const token = searchParams.get("token");
  const [state, setState] = useState<State>("loading");

  useEffect(() => {
    if (!token) {
      setState("invalid");
      return;
    }

    auth.verifyEmail(token)
      .then(() => setState("success"))
      .catch((err: Error) => {
        const msg = err.message.toLowerCase();
        if (msg.includes("expired") || msg.includes("used") || msg.includes("invalid")) {
          setState("expired");
        } else {
          setState("invalid");
        }
      });
  }, [token]);

  if (state === "loading") {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardContent className="pt-8 pb-6">
            <Loader2 size={40} className="mx-auto mb-4 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">Verifying your email…</p>
          </CardContent>
        </Card>
      </main>
    );
  }

  if (state === "success") {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <CheckCircle size={40} className="mx-auto mb-2 text-green-500" />
            <CardTitle className="text-2xl">Email verified!</CardTitle>
            <CardDescription>
              Your email address has been confirmed. You&apos;re all set.
            </CardDescription>
          </CardHeader>
          <CardFooter className="justify-center">
            <Button asChild>
              <Link href="/feed">Go to Klar</Link>
            </Button>
          </CardFooter>
        </Card>
      </main>
    );
  }

  if (state === "expired") {
    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <XCircle size={40} className="mx-auto mb-2 text-destructive" />
            <CardTitle className="text-2xl">Link expired</CardTitle>
            <CardDescription>
              This verification link has expired or already been used.
              Request a new one below.
            </CardDescription>
          </CardHeader>
          <CardFooter className="flex-col gap-2">
            <Button asChild className="w-full">
              <Link href="/resend-verification">Resend verification email</Link>
            </Button>
            <Button asChild variant="ghost" className="w-full">
              <Link href="/login">Back to sign in</Link>
            </Button>
          </CardFooter>
        </Card>
      </main>
    );
  }

  // invalid / no token
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm text-center">
        <CardHeader>
          <XCircle size={40} className="mx-auto mb-2 text-destructive" />
          <CardTitle className="text-2xl">Invalid link</CardTitle>
          <CardDescription>
            This verification link is invalid. Please check your email for the
            correct link or request a new one.
          </CardDescription>
        </CardHeader>
        <CardFooter className="flex-col gap-2">
          <Button asChild className="w-full">
            <Link href="/resend-verification">Resend verification email</Link>
          </Button>
          <Button asChild variant="ghost" className="w-full">
            <Link href="/login">Back to sign in</Link>
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
    <Suspense 
      fallback={
        <main className="flex min-h-screen items-center justify-center bg-background p-4">
          <Card className="w-full max-w-sm text-center">
            <CardContent className="pt-8 pb-6">
              <Loader2 size={40} className="mx-auto mb-4 animate-spin text-muted-foreground" />
              <p className="text-sm text-muted-foreground">Verifying your email…</p>
            </CardContent>
          </Card>
        </main>
      }
    >
      <VerifyEmailContent />
    </Suspense>
  );
}