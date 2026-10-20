"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";

import { useAuth } from "@/lib/auth-context";
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
function registerSchema(t: ReturnType<typeof useTranslations<"auth">>) {
  return z
    .object({
      username: z
        .string()
        .min(3, t("register.usernameMin"))
        .max(30, t("register.usernameMax"))
        .regex(/^[a-zA-Z0-9_]+$/, t("register.usernameChars")),
      email: z.string().email(t("invalidEmail")),
      password: z
        .string()
        .min(8, t("passwordMin"))
        .max(100, t("passwordTooLong")),
      confirmPassword: z.string(),
      acceptTerms: z.boolean().refine((val) => val === true, {
        message: t("register.acceptTerms"),
      }),
    })
    .refine((data) => data.password === data.confirmPassword, {
      message: t("passwordsDontMatch"),
      path: ["confirmPassword"],
    });
}

type RegisterValues = z.infer<ReturnType<typeof registerSchema>>;

// ── Page ──────────────────────────────────────────────────────────────────────

export default function RegisterPage() {
  const { register } = useAuth();
  const router = useRouter();
  const t = useTranslations("auth");
  const schema = useMemo(() => registerSchema(t), [t]);
  const [error, setError] = useState<string | null>(null);
  const [registered, setRegistered] = useState(false);

  const form = useForm<RegisterValues>({
    resolver: zodResolver(schema),
    defaultValues: {
      username: "",
      email: "",
      password: "",
      confirmPassword: "",
      acceptTerms: false,
    },
  });

  const onSubmit = async (values: RegisterValues) => {
    setError(null);
    try {
      // acceptTerms is enforced server-side too (POST /auth/register
      // rejects the request with 400 if it isn't true) -- this client
      // check is a UX nicety, not the actual guarantee.
      await register(values.username, values.email, values.password, values.acceptTerms);
      setRegistered(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("register.failed"));
    }
  };

  // After registration — show email verification prompt
  if (registered) {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <CardTitle className="text-2xl">{t("checkEmail")}</CardTitle>
            <CardDescription>
              {t.rich("register.sentLink", {
                email: form.getValues("email"),
                addr: (chunks) => <span className="font-medium text-foreground">{chunks}</span>,
              })}
            </CardDescription>
          </CardHeader>
          <CardContent className="text-sm text-muted-foreground">
            {t("register.untilVerified")}
          </CardContent>
          <CardFooter className="justify-center">
            <Button onClick={() => router.push("/feed")}>
              {t("register.continue")}
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
          <CardTitle className="text-2xl">{t("register.title")}</CardTitle>
          <CardDescription>{t("register.subtitle")}</CardDescription>
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
                name="username"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("username")}</FormLabel>
                    <FormControl>
                      <Input
                        placeholder={t("username")}
                        autoComplete="username"
                        autoCapitalize="none"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="email"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>{t("email")}</FormLabel>
                    <FormControl>
                      <Input
                        type="email"
                        placeholder="you@example.com"
                        autoComplete="email"
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
                        autoComplete="new-password"
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
                    <FormLabel>{t("register.confirmPassword")}</FormLabel>
                    <FormControl>
                      <Input
                        type="password"
                        placeholder="••••••••"
                        autoComplete="new-password"
                        {...field}
                      />
                    </FormControl>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <FormField
                control={form.control}
                name="acceptTerms"
                render={({ field }) => (
                  <FormItem>
                    <div className="flex items-start gap-2">
                      <FormControl>
                        <input
                          type="checkbox"
                          checked={field.value}
                          onChange={(e) => field.onChange(e.target.checked)}
                          className="mt-0.5 h-4 w-4 shrink-0 rounded border-input"
                        />
                      </FormControl>
                      <label className="text-sm text-muted-foreground leading-snug">
                        {t.rich("register.terms", {
                          terms: (chunks) => (
                            <Link href="/nutzungsbedingungen" className="underline" target="_blank">
                              {chunks}
                            </Link>
                          ),
                          privacy: (chunks) => (
                            <Link href="/datenschutz" className="underline" target="_blank">
                              {chunks}
                            </Link>
                          ),
                        })}
                      </label>
                    </div>
                    <FormMessage />
                  </FormItem>
                )}
              />

              <Button
                type="submit"
                className="w-full"
                disabled={form.formState.isSubmitting}
              >
                {form.formState.isSubmitting ? t("register.creating") : t("register.create")}
              </Button>
            </form>
          </Form>
        </CardContent>

        <CardFooter className="justify-center gap-1 text-sm text-muted-foreground">
          {t.rich("register.haveAccount", {
            link: (chunks) => (
              <Link
                href="/login"
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
