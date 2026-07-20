"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, Camera } from "lucide-react";
import Image from "next/image";
import { useAuth } from "@/lib/auth-context";
import { users as usersApi } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SmartBackButton } from '@/components/SmartBackButton';
import { getMediaUrl } from "@/lib/utils/media";
import { ENV } from '@/env';

const API_URL = ENV.API_URL;

export default function EditProfilePage() {
  const { user, loading: authLoading, refreshUser } = useAuth();
  const router = useRouter();

  const [username, setUsername] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [bio, setBio] = useState("");
  const [avatarPreview, setAvatarPreview] = useState<string | null>(null);
  const [avatarFile, setAvatarFile] = useState<File | null>(null);
  const [saving, setSaving] = useState(false);
  const [uploadingAvatar, setUploadingAvatar] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const fileInputRef = useRef<HTMLInputElement>(null);

  // Redirect if not logged in
  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  // Pre-fill from current profile
  useEffect(() => {
    if (!user) return;
    setUsername(user.username ?? "");
    setDisplayName(user.display_name ?? "");
    setBio(user.bio ?? "");
  }, [user]);

  // Cooldown Calculation (14 days)
  const COOLDOWN_DAYS = 14;
  let isOnCooldown = false;
  let unlockDateStr = "";

  // Cast user to safely access the new field without requiring an immediate api.ts rewrite
  const userWithCooldown = user as (typeof user & { username_changed_at?: string | null });

  if (userWithCooldown?.username_changed_at) {
    const changedAt = new Date(userWithCooldown.username_changed_at);
    const unlockDate = new Date(changedAt.getTime() + COOLDOWN_DAYS * 24 * 60 * 60 * 1000);
    
    if (new Date() < unlockDate) {
      isOnCooldown = true;
      unlockDateStr = unlockDate.toLocaleDateString(undefined, {
        year: 'numeric', month: 'long', day: 'numeric'
      });
    }
  }

  const handleAvatarChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    if (file.size > 5 * 1024 * 1024) {
      setError("Avatar must be under 5MB");
      return;
    }
    setAvatarFile(file);
    setAvatarPreview(URL.createObjectURL(file));
  };

  const handleSave = async () => {
    if (saving || !user) return;
    setError(null);
    setSuccess(false);

    const newUsername = username.trim();

    // Basic frontend validation
    if (newUsername.length < 3 || newUsername.length > 30) {
      setError("Username must be between 3 and 30 characters");
      return;
    }

    setSaving(true);

    try {
      // Upload avatar first if changed
      if (avatarFile) {
        setUploadingAvatar(true);
        await usersApi.uploadAvatar(avatarFile);
        setUploadingAvatar(false);
        setAvatarFile(null);
      }

      // Update profile (Only send username if it actually changed to avoid triggering cooldown unnecessarily)
      const usernamePayload = newUsername.toLowerCase() !== user.username.toLowerCase() ? newUsername : null;

      await usersApi.updateProfile(
        usernamePayload,
        displayName.trim() || null,
        bio.trim() || null
      );

      // Refresh the cached user so other pages (e.g. "is this my own
      // profile") don't keep comparing against stale pre-save data.
      await refreshUser();

      setSuccess(true);
      
      // Redirect to the new username URL if it changed, otherwise back to the current one
      setTimeout(() => {
        router.push(`/users/${newUsername || user.username}`);
      }, 800);

    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save profile");
      setUploadingAvatar(false);
    } finally {
      setSaving(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="sticky top-0 z-10 flex h-14 items-center justify-between border-b border-border bg-background/80 px-4 backdrop-blur">
        <div className="flex items-center gap-3">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Edit Profile</span>
        </div>
        <div>
          <Button
            size="sm"
            onClick={handleSave}
            disabled={saving}
          >
            {saving
              ? uploadingAvatar
                ? "Uploading…"
                : "Saving…"
              : success
              ? "Saved!"
              : "Save"}
          </Button>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-6 px-4 py-6">
        {error && (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        {/* Avatar upload */}
        <div className="flex flex-col items-center gap-3">
          <button
            onClick={() => fileInputRef.current?.click()}
            className="group relative h-24 w-24 overflow-hidden rounded-full bg-muted focus:outline-none"
            aria-label="Change avatar"
          >
            {user.avatar_url ? (
              <Image
                src={getMediaUrl(user.avatar_url)}
                alt="Avatar"
                fill
                className="object-cover"
                unoptimized
              />
            ) : (
              <span className="flex h-full w-full items-center justify-center text-3xl font-bold uppercase">
                {user.username[0]}
              </span>
            )}
            {/* Overlay on hover */}
            <div className="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 transition-opacity group-hover:opacity-100">
              <Camera size={24} className="text-white" />
            </div>
          </button>
          <button
            onClick={() => fileInputRef.current?.click()}
            className="text-sm font-medium text-foreground underline-offset-4 hover:underline"
          >
            Change photo
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={handleAvatarChange}
          />
        </div>

        {/* Username */}
        <div className="space-y-2">
          <Label htmlFor="username">Username</Label>
          <Input
            id="username"
            value={username}
            onChange={(e) => setUsername(e.target.value.replace(/[^a-zA-Z0-9_.-]/g, ''))} // Basic strict character filtering
            placeholder="username"
            maxLength={30}
            disabled={isOnCooldown || saving}
            className={isOnCooldown ? "opacity-60 bg-muted cursor-not-allowed" : ""}
          />
          {isOnCooldown ? (
            <p className="text-xs text-amber-500 font-medium">
              You recently changed your username. You can change it again on {unlockDateStr}.
            </p>
          ) : (
            <p className="text-xs text-muted-foreground">
              You can change your username once every 14 days.
            </p>
          )}
        </div>

        {/* Display name */}
        <div className="space-y-2">
          <Label htmlFor="display-name">Display name</Label>
          <Input
            id="display-name"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="Your name"
            maxLength={50}
            disabled={saving}
          />
          <p className="text-xs text-muted-foreground">
            {displayName.length}/50
          </p>
        </div>

        {/* Bio */}
        <div className="space-y-2">
          <Label htmlFor="bio">Bio</Label>
          <textarea
            id="bio"
            value={bio}
            onChange={(e) => setBio(e.target.value)}
            placeholder="Tell people a little about yourself…"
            maxLength={500}
            rows={4}
            disabled={saving}
            className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring disabled:opacity-50 disabled:cursor-not-allowed"
          />
          <p className="text-xs text-muted-foreground">
            {bio.length}/500
          </p>
        </div>
      </main>
    </div>
  );
}