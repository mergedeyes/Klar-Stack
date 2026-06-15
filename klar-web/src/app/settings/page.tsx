"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, ChevronRight, KeyRound, Trash2, UserPen } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from '@/components/SmartBackButton';

const sections = [
  {
    href: "/settings/profile",
    icon: UserPen,
    label: "Edit profile",
    description: "Change your avatar, display name, and bio",
  },
  {
    href: "/settings/password",
    icon: KeyRound,
    label: "Change password",
    description: "Update your password",
  },
  {
    href: "/settings/account",
    icon: Trash2,
    label: "Account",
    description: "Log out or delete your account",
    destructive: true,
  },
];

export default function SettingsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Account</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-4">
        <div className="overflow-hidden rounded-xl border border-border">
          {sections.map((section, i) => (
            <button
              key={section.href}
              onClick={() => router.push(section.href)}
              className={`flex w-full items-center gap-4 px-4 py-4 text-left transition-colors hover:bg-muted/50 ${
                i < sections.length - 1 ? "border-b border-border" : ""
              }`}
            >
              <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full ${
                section.destructive ? "bg-destructive/10 text-destructive" : "bg-muted"
              }`}>
                <section.icon size={18} />
              </div>
              <div className="flex-1">
                <p className={`text-sm font-medium ${section.destructive ? "text-destructive" : ""}`}>
                  {section.label}
                </p>
                <p className="text-xs text-muted-foreground">{section.description}</p>
              </div>
              <ChevronRight size={16} className="text-muted-foreground" />
            </button>
          ))}
        </div>
      </main>
    </div>
  );
}