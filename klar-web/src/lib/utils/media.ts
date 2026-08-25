// lib/utils/media.ts
import { ENV } from '@/env';
const STORAGE_URL = ENV.STORAGE_URL

export function getMediaUrl(key: string | null | undefined): string {
  if (!key) return "";
  // Matches both http and https: local dev's LocalStorage backend returns
  // plain http URLs (no TLS on localhost), and those still need to pass
  // through unchanged here rather than getting STORAGE_URL prepended a
  // second time on top of an already-complete URL.
  if (key.startsWith("http")) return key;
  if (!STORAGE_URL) return key;

  const baseUrl = STORAGE_URL.replace(/\/$/, "");
  const cleanKey = key.startsWith("/") ? key.substring(1) : key;

  return `${baseUrl}/${cleanKey}`;
}
