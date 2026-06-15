"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, LogOut, Trash2 } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { users as usersApi } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from '@/components/SmartBackButton';

export default function AccountSettingsPage() {
  const { user, loading: authLoading, logout } = useAuth();
  const router = useRouter();
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [deleteInput, setDeleteInput] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const handleLogout = async () => {
    await logout();
    router.push("/login");
  };

  const handleDelete = async () => {
    if (!user || deleting) return;
    setDeleting(true);
    setError(null);
    try {
      await usersApi.deleteAccount();
      await logout();
      router.push("/login");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete account");
      setDeleting(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Account</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-4 px-4 py-6">
        {error && (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* Log out */}
        <div className="overflow-hidden rounded-xl border border-border">
          <button
            onClick={handleLogout}
            className="flex w-full items-center gap-4 px-4 py-4 text-left transition-colors hover:bg-muted/50"
          >
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
              <LogOut size={18} />
            </div>
            <div>
              <p className="text-sm font-medium">Log out</p>
              <p className="text-xs text-muted-foreground">Sign out of your account</p>
            </div>
          </button>
        </div>

        {/* Delete account */}
        <div className="overflow-hidden rounded-xl border border-destructive/30">
          {!showDeleteConfirm ? (
            <button
              onClick={() => setShowDeleteConfirm(true)}
              className="flex w-full items-center gap-4 px-4 py-4 text-left transition-colors hover:bg-destructive/5"
            >
              <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-destructive/10 text-destructive">
                <Trash2 size={18} />
              </div>
              <div>
                <p className="text-sm font-medium text-destructive">Delete account</p>
                <p className="text-xs text-muted-foreground">
                  Permanently delete your account and all data
                </p>
              </div>
            </button>
          ) : (
            <div className="space-y-4 p-4">
              <div>
                <p className="text-sm font-semibold text-destructive">Delete your account?</p>
                <p className="mt-1 text-sm text-muted-foreground">
                  This will permanently delete all your posts, comments, and data.
                  This cannot be undone.
                </p>
              </div>
              <div className="space-y-2">
                <p className="text-xs text-muted-foreground">
                  Type <span className="font-mono font-semibold text-foreground">{user.username}</span> to confirm
                </p>
                <input
                  value={deleteInput}
                  onChange={(e) => setDeleteInput(e.target.value)}
                  placeholder={user.username}
                  className="w-full rounded-md border border-border bg-transparent px-3 py-2 text-sm outline-none focus:border-destructive"
                />
              </div>
              <div className="flex gap-2">
                <Button
                  variant="destructive"
                  className="flex-1"
                  onClick={handleDelete}
                  disabled={deleteInput !== user.username || deleting}
                >
                  {deleting ? "Deleting…" : "Delete account"}
                </Button>
                <Button
                  variant="outline"
                  className="flex-1"
                  onClick={() => { setShowDeleteConfirm(false); setDeleteInput(""); }}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}