// lib/utils/media.ts
const STORAGE_URL = process.env.NEXT_PUBLIC_STORAGE_URL ?? "https://cdn.klarsocial.de";

export function getMediaUrl(key: string | null | undefined): string {
  if (!key) return "";
  if (key.startsWith("http")) return key;

  const baseUrl = STORAGE_URL.replace(/\/$/, "");
  const cleanKey = key.startsWith("/") ? key.substring(1) : key;

  return `${baseUrl}/${cleanKey}`;
}