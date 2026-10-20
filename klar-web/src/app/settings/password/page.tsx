"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useAuth } from "@/lib/auth-context";
import { users as usersApi } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { Input } from "@/components/ui/input";
import { SmartBackButton } from '@/components/SmartBackButton';

function passwordSchema(t: ReturnType<typeof useTranslations<"password">>, ta: ReturnType<typeof useTranslations<"auth">>) {
  return z.object({
    currentPassword: z.string().min(1, t("currentRequired")),
    newPassword: z.string().min(8, ta("passwordMin")).max(100),
    confirmPassword: z.string(),
  }).refine((d) => d.newPassword === d.confirmPassword, {
    message: ta("passwordsDontMatch"),
    path: ["confirmPassword"],
  }).refine((d) => d.currentPassword !== d.newPassword, {
    message: t("mustDiffer"),
    path: ["newPassword"],
  });
}

type Values = z.infer<ReturnType<typeof passwordSchema>>;

export default function ChangePasswordPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const t = useTranslations("password");
  const ta = useTranslations("auth");
  const tc = useTranslations("common");
  const tacc = useTranslations("account");
  const ts = useTranslations("settings");
  const schema = useMemo(() => passwordSchema(t, ta), [t, ta]);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { currentPassword: "", newPassword: "", confirmPassword: "" },
  });

  const onSubmit = async (values: Values) => {
    setError(null);
    try {
      await usersApi.changePassword(values.currentPassword, values.newPassword);
      setSuccess(true);
      setTimeout(() => router.push("/settings"), 1500);
    } catch (err) {
      setError(err instanceof Error ? err.message : t("failed"));
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-safe z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton />
          <span className="font-semibold">{tacc("title")}</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-6">
        <Form {...form}>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            {error && (
              <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {error}
              </div>
            )}
            {success && (
              <div className="rounded-md bg-green-500/10 px-3 py-2 text-sm text-green-600 dark:text-green-400">
                {t("changed")}
              </div>
            )}

            <FormField
              control={form.control}
              name="currentPassword"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{t("current")}</FormLabel>
                  <FormControl>
                    <Input type="password" placeholder="••••••••" autoComplete="current-password" {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <FormField
              control={form.control}
              name="newPassword"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>{ta("reset.newPassword")}</FormLabel>
                  <FormControl>
                    <Input type="password" placeholder="••••••••" autoComplete="new-password" {...field} />
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
                  <FormLabel>{ta("reset.confirmNewPassword")}</FormLabel>
                  <FormControl>
                    <Input type="password" placeholder="••••••••" autoComplete="new-password" {...field} />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />

            <Button type="submit" className="w-full" disabled={form.formState.isSubmitting || success}>
              {form.formState.isSubmitting ? tc("saving") : ts("changePassword")}
            </Button>
          </form>
        </Form>
      </main>
    </div>
  );
}