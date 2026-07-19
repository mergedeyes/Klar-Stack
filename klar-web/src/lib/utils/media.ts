// lib/utils/media.ts
import { ENV } from '../env';
const STORAGE_URL = ENV.API_URL

export function getMediaUrl(key: string | null | undefined): string {
  if (!key) return "";
  if (key.startsWith("https")) return key;

  const baseUrl = STORAGE_URL.replace(/\/$/, "");
  const cleanKey = key.startsWith("/") ? key.substring(1) : key;

  return `${baseUrl}/${cleanKey}`;
}