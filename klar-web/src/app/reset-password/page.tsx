"use client";

import { useMemo, useState, Suspense } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { auth } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { Input } from "@/components/ui/input";

function passwordSchema(t: ReturnType<typeof useTranslations<"auth">>) {
  return z.object({
    password: z.string().min(8, t("passwordMin")).max(100),
    confirmPassword: z.string(),
  }).refine((d) => d.password === d.confirmPassword, {
    message: t("passwordsDontMatch"),
    path: ["confirmPassword"],
  });
}
type Values = z.infer<ReturnType<typeof passwordSchema>>;

// 1. Deine eigentliche Logik wandert in diese innere Komponente
function ResetPasswordForm() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const token = searchParams.get("token");
  const t = useTranslations("auth");
  const schema = useMemo(() => passwordSchema(t), [t]);

  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { password: "", confirmPassword: "" },
  });

  // Derived rather than set from an effect: the link is unusable for as
  // long as it has no token.
  const shownError = error ?? (token ? null : t("reset.invalidLink"));

  const onSubmit = async (values: Values) => {
    if (!token) return;
    setError(null);
    try {
      await auth.resetPassword(token, values.password);
      setSuccess(true);
      setTimeout(() => router.push("/login"), 2000);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("reset.failed"));
    }
  };

  if (success) {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <CardTitle className="text-2xl">{t("reset.doneTitle")}</CardTitle>
            <CardDescription>{t("reset.doneText")}</CardDescription>
          </CardHeader>
        </Card>
      </main>
    );
  }

  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm">
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">{t("reset.title")}</CardTitle>
          <CardDescription>{t("reset.intro")}</CardDescription>
        </CardHeader>

        <CardContent>
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
              {shownError && (
                <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
                  {shownError}
                </div>
              )}

              <FormField
                control={form.control}
                name="password"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("reset.newPassword")}</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="••••••••"
                        autoComplete="new-password"
                        disabled={!token}
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="confirmPassword"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("reset.confirmNewPassword")}</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="••••••••"
                        autoComplete="new-password"
                        disabled={!token}
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
                disabled={!token || form.formState.isSubmitting}
              >
                {form.formState.isSubmitting ? t("reset.resetting") : t("reset.submit")}
              </Button>
            </form>
          </Form>
        </CardContent>

        <CardFooter className="justify-center text-sm text-muted-foreground">
          <Link href="/login" className="font-medium text-foreground underline-offset-4 hover:underline">
            {t("backToSignIn")}
          </Link>
        </CardFooter>
      </Card>
    </main>
  );
}

function Loading() {
  const tc = useTranslations("common");
  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <div className="text-muted-foreground">{tc("loading")}</div>
    </main>
  );
}

// 2. Deine exportierte Seite nutzt Suspense, um den Build-Fehler zu verhindern
export default function ResetPasswordPage() {
  return (
    // Der Fallback hält das Layout stabil, während die URL ausgelesen wird
    <Suspense fallback={<Loading />}>
      <ResetPasswordForm />
    </Suspense>
  );
}
