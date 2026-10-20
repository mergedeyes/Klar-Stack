"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";

import { useAuth } from "@/lib/auth-context";
import { auth, HttpError } from "@/lib/api";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form";
import { Input } from "@/components/ui/input";

// ── Schema ────────────────────────────────────────────────────────────────────

// Built per render language, so the validation messages are translated.
function loginSchema(t: ReturnType<typeof useTranslations<"auth">>) {
  return z.object({
    // An email address or a username; the server tells them apart.
    login: z.string().trim().min(1, t("login.enterLogin")),
    password: z.string().min(1, t("login.passwordRequired")),
  });
}

type LoginValues = z.infer<ReturnType<typeof loginSchema>>;

// ── Page ──────────────────────────────────────────────────────────────────────

export default function LoginPage() {
  const { login } = useAuth();
  const router = useRouter();
  const t = useTranslations("auth");
  const tc = useTranslations("common");
  const schema = useMemo(() => loginSchema(t), [t]);
  const [error, setError] = useState<string | null>(null);
  // Set when the account is locked after a suspected takeover (423): the
  // credentials, to request a new link, and the outcome of that request.
  const [locked, setLocked] = useState<LoginValues | null>(null);
  const [resend, setResend] = useState<{ busy: boolean; message: string | null; error: boolean }>({
    busy: false,
    message: null,
    error: false,
  });

  const form = useForm<LoginValues>({
    resolver: zodResolver(schema),
    defaultValues: { login: "", password: "" },
  });

  const onSubmit = async (values: LoginValues) => {
    setError(null);
    try {
      await login(values.login, values.password);
      router.push("/feed");
    } catch (err) {
      if (err instanceof HttpError && err.status === 423) {
        setLocked(values);
        setResend({ busy: false, message: null, error: false });
        return;
      }
      setError(err instanceof Error ? err.message : t("login.failed"));
    }
  };

  const resendLink = async () => {
    if (!locked) return;
    setResend({ busy: true, message: null, error: false });
    try {
      const res = await auth.resendLockLink(locked.login, locked.password);
      setResend({ busy: false, message: res.message, error: false });
    } catch (err) {
      setResend({ busy: false, message: err instanceof Error ? err.message : t("login.couldntSendLink"), error: true });
    }
  };

  if (locked) {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm">
          <CardHeader className="text-center">
            <CardTitle className="text-2xl">{t("login.lockedTitle")}</CardTitle>
            <CardDescription>{t("login.lockedSubtitle")}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <p>{t("login.lockedWhy")}</p>
            <p>{t("login.lockedWhat")}</p>
            {resend.message && (
              <div
                className={`rounded-md px-3 py-2 ${resend.error ? "bg-destructive/10 text-destructive" : "bg-muted"}`}
                role="status"
              >
                {resend.message}
              </div>
            )}
            <Button className="w-full" variant="outline" onClick={resendLink} disabled={resend.busy}>
              {resend.busy ? tc("sending") : t("login.sendAgain")}
            </Button>
            <p className="text-muted-foreground">
              {t.rich("login.noEmail", {
                mail: (chunks) => (
                  <a href="mailto:kontakt@klarsocial.eu" className="underline">
                    {chunks}
                  </a>
                ),
              })}
            </p>
          </CardContent>
          <CardFooter className="justify-center">
            <Button variant="ghost" size="sm" onClick={() => setLocked(null)}>
              {t("backToSignIn")}
            </Button>
          </CardFooter>
        </Card>
      </main>
    );
  }

  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm">
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">Klar</CardTitle>
          <CardDescription>{t("login.subtitle")}</CardDescription>
        </CardHeader>

        <CardContent>
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              {error && (
                <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  {error}
                </div>
              )}

              <FormField
                control={form.control}
                name="login"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("login.emailOrUsername")}</FormLabel>
                    <FormControl>
                      <Input
                        type="text"
                        placeholder="you@example.com"
                        autoComplete="username"
                        autoCapitalize="none"
                        autoCorrect="off"
                        spellCheck={false}
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="password"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("password")}</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="••••••••"
                        autoComplete="current-password"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <Button
                type="submit"
                className="w-full"
                disabled={form.formState.isSubmitting}
              >
                {form.formState.isSubmitting ? t("login.signingIn") : t("login.signIn")}
              </Button>
            </form>
          </Form>

          <div className="mt-4 text-center text-sm">
            <Link
              href="/forgot-password"
              className="text-muted-foreground underline-offset-4 hover:underline"
            >
              {t("login.forgotPassword")}
            </Link>
          </div>
        </CardContent>

        <CardFooter className="justify-center gap-1 text-sm text-muted-foreground">
          {t.rich("login.noAccount", {
            link: (chunks) => (
              <Link
                href="/register"
                className="font-medium text-foreground underline-offset-4 hover:underline"
              >
                {chunks}
              </Link>
            ),
          })}
        </CardFooter>
      </Card>
    </main>
  );
}
