"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { auth } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Form, FormControl, FormField, FormItem, FormLabel, FormMessage } from "@/components/ui/form";
import { Input } from "@/components/ui/input";

function emailSchema(t: ReturnType<typeof useTranslations<"auth">>) {
  return z.object({
    email: z.string().email(t("invalidEmail")),
  });
}
type Values = z.infer<ReturnType<typeof emailSchema>>;

export default function ResendVerificationPage() {
  const t = useTranslations("auth");
  const tc = useTranslations("common");
  const schema = useMemo(() => emailSchema(t), [t]);
  const [submitted, setSubmitted] = useState(false);

  const form = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { email: "" },
  });

  const onSubmit = async (values: Values) => {
    try {
      await auth.resendVerification(values.email);
    } catch {
      // Intentionally ignored — never reveal if email exists
    }
    setSubmitted(true);
  };

  if (submitted) {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm text-center">
          <CardHeader>
            <CardTitle className="text-2xl">{t("checkEmail")}</CardTitle>
            <CardDescription>{t("resend.submitted")}</CardDescription>
          </CardHeader>
          <CardFooter className="justify-center">
            <Link href="/login" className="text-sm font-medium text-foreground underline-offset-4 hover:underline">
              {t("backToSignIn")}
            </Link>
          </CardFooter>
        </Card>
      </main>
    );
  }

  return (
    <main className="flex flex-1 items-center justify-center bg-background p-4">
      <Card className="w-full max-w-sm">
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">{t("resend.title")}</CardTitle>
          <CardDescription>{t("resend.intro")}</CardDescription>
        </CardHeader>

        <CardContent>
          <Form {...form}>
            <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
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
              <Button type="submit" className="w-full" disabled={form.formState.isSubmitting}>
                {form.formState.isSubmitting ? tc("sending") : t("resend.send")}
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
