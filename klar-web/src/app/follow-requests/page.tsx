"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { UserCheck } from "lucide-react";
import Link from "next/link";
import { useAuth } from "@/lib/auth-context";
import { followRequestsApi, type FollowRequest } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { getMediaUrl } from "@/lib/utils/media";

export default function FollowRequestsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [requests, setRequests] = useState<FollowRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyUsername, setBusyUsername] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    followRequestsApi.list()
      .then(setRequests)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load requests"))
      .finally(() => setLoading(false));
  }, [user]);

  const handleAccept = async (req: FollowRequest) => {
    setBusyUsername(req.requester_username);
    try {
      await followRequestsApi.accept(req.requester_username);
      setRequests((prev) => prev.filter((r) => r.requester_username !== req.requester_username));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to accept request");
    } finally {
      setBusyUsername(null);
    }
  };

  const handleReject = async (req: FollowRequest) => {
    setBusyUsername(req.requester_username);
    try {
      await followRequestsApi.reject(req.requester_username);
      setRequests((prev) => prev.filter((r) => r.requester_username !== req.requester_username));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to reject request");
    } finally {
      setBusyUsername(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Follow Requests</span>
      </header>

      <main className="mx-auto max-w-lg px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        {loading && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">
            Loading…
          </div>
        )}

        {!loading && requests.length === 0 && (
          <div className="py-16 text-center">
            <UserCheck size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No pending follow requests</p>
          </div>
        )}

        <div className="space-y-1">
          {requests.map((req) => (
            <div key={req.requester_id} className="flex items-center gap-3 rounded-xl p-3 hover:bg-muted/50">
              <Link href={`/users/${req.requester_username}`} className="flex flex-1 items-center gap-3 min-w-0">
                <div className="h-11 w-11 shrink-0 overflow-hidden rounded-full bg-muted flex items-center justify-center">
                  {req.requester_avatar_url ? (
                    <img
                      src={getMediaUrl(req.requester_avatar_url)}
                      alt={req.requester_username}
                      className="h-full w-full object-cover"
                    />
                  ) : (
                    <span className="font-bold uppercase">{req.requester_username.charAt(0)}</span>
                  )}
                </div>
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold">{req.requester_username}</p>
                  {req.requester_display_name && (
                    <p className="truncate text-xs text-muted-foreground">{req.requester_display_name}</p>
                  )}
                </div>
              </Link>
              <div className="flex shrink-0 gap-2">
                <Button
                  size="sm"
                  onClick={() => handleAccept(req)}
                  disabled={busyUsername === req.requester_username}
                >
                  Accept
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => handleReject(req)}
                  disabled={busyUsername === req.requester_username}
                >
                  Decline
                </Button>
              </div>
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}
