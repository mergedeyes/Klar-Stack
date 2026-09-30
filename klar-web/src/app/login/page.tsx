"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
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

const loginSchema = z.object({
  email: z.string().email("Invalid email address"),
  password: z.string().min(1, "Password is required"),
});

type LoginValues = z.infer<typeof loginSchema>;

// ── Page ──────────────────────────────────────────────────────────────────────

export default function LoginPage() {
  const { login } = useAuth();
  const router = useRouter();
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
    resolver: zodResolver(loginSchema),
    defaultValues: { email: "", password: "" },
  });

  const onSubmit = async (values: LoginValues) => {
    setError(null);
    try {
      await login(values.email, values.password);
      router.push("/feed");
    } catch (err) {
      if (err instanceof HttpError && err.status === 423) {
        setLocked(values);
        setResend({ busy: false, message: null, error: false });
        return;
      }
      setError(err instanceof Error ? err.message : "Login failed");
    }
  };

  const resendLink = async () => {
    if (!locked) return;
    setResend({ busy: true, message: null, error: false });
    try {
      const res = await auth.resendLockLink(locked.email, locked.password);
      setResend({ busy: false, message: res.message, error: false });
    } catch (err) {
      setResend({ busy: false, message: err instanceof Error ? err.message : "Couldn't send the link", error: true });
    }
  };

  if (locked) {
    return (
      <main className="flex flex-1 items-center justify-center bg-background p-4">
        <Card className="w-full max-w-sm">
          <CardHeader className="text-center">
            <CardTitle className="text-2xl">Your account is locked</CardTitle>
            <CardDescription>For your protection</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <p>
              We noticed activity on your account that suggests someone else has been using it, so we signed it out
              everywhere and locked it.
            </p>
            <p>
              We sent a link to your email address. Set a new password with it to unlock your account, and use one
              you don&apos;t use anywhere else.
            </p>
            {resend.message && (
              <div
                className={`rounded-md px-3 py-2 ${resend.error ? "bg-destructive/10 text-destructive" : "bg-muted"}`}
                role="status"
              >
                {resend.message}
              </div>
            )}
            <Button className="w-full" variant="outline" onClick={resendLink} disabled={resend.busy}>
              {resend.busy ? "Sending…" : "Send the link again"}
            </Button>
            <p className="text-muted-foreground">
              No email? Check your spam folder, or write to{" "}
              <a href="mailto:kontakt@klarsocial.eu" className="underline">
                kontakt@klarsocial.eu
              </a>
              .
            </p>
          </CardContent>
          <CardFooter className="justify-center">
            <Button variant="ghost" size="sm" onClick={() => setLocked(null)}>
              Back to sign in
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
          <CardDescription>Sign in to your account</CardDescription>
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
                name="email"
                render={({ field }) => (
                  <FormItem>
                    <FormLabel>Email</FormLabel>
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
                    <FormLabel>Password</FormLabel>
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
                {form.formState.isSubmitting ? "Signing in…" : "Sign in"}
              </Button>
            </form>
          </Form>

          <div className="mt-4 text-center text-sm">
            <Link
              href="/forgot-password"
              className="text-muted-foreground underline-offset-4 hover:underline"
            >
              Forgot password?
            </Link>
          </div>
        </CardContent>

        <CardFooter className="justify-center text-sm text-muted-foreground">
          No account?&nbsp;
          <Link
            href="/register"
            className="font-medium text-foreground underline-offset-4 hover:underline"
          >
            Create one
          </Link>
        </CardFooter>
      </Card>
    </main>
  );
}
