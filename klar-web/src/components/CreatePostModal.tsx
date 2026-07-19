"use client";

import { useCallback, useRef, useState } from "react";
import Image from "next/image";
import { ImagePlus, Loader2, X } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { tokens } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { ENV } from '../env';

const API_URL = ENV.API_URL;

interface CreatePostModalProps {
  onClose: () => void;
  onCreated: () => void;
}

export default function CreatePostModal({ onClose, onCreated }: CreatePostModalProps) {
  const { user } = useAuth();

  const [preview, setPreview] = useState<string | null>(null);
  const [file, setFile] = useState<File | null>(null);
  const [caption, setCaption] = useState("");
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  // Close on Escape
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  }, [onClose]);

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (!f) return;
    if (f.size > 10 * 1024 * 1024) {
      setError("Image must be under 10MB");
      return;
    }
    setFile(f);
    setPreview(URL.createObjectURL(f));
    setError(null);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    const f = e.dataTransfer.files[0];
    if (!f) return;
    if (!f.type.startsWith("image/")) { setError("Only image files are supported"); return; }
    if (f.size > 10 * 1024 * 1024) { setError("Image must be under 10MB"); return; }
    setFile(f);
    setPreview(URL.createObjectURL(f));
    setError(null);
  };

  const handleSubmit = async () => {
    if (!file || uploading) return;
    setUploading(true);
    setError(null);

    try {
      const form = new FormData();
      form.append("image", file);
      if (caption.trim()) form.append("caption", caption.trim());

      const token = tokens.getAccess();
      const res = await fetch(`${API_URL}/posts/upload`, {
        method: "POST",
        credentials: 'include',
        headers: {
          'Authorization': `Bearer ${token}` // <-- Diese Zeile einfügen
        },
        body: form,
      });

      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? "Upload failed");

      onCreated();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Upload failed");
    } finally {
      setUploading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      onKeyDown={handleKeyDown}
    >
      <div className="relative w-full max-w-lg overflow-hidden rounded-xl bg-background shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <span className="font-semibold">New post</span>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground" aria-label="Close">
            <X size={20} />
          </button>
        </div>

        {/* Image picker */}
        {!preview ? (
          <div
            className="flex cursor-pointer flex-col items-center justify-center gap-3 p-12 transition-colors hover:bg-muted/30"
            onClick={() => fileInputRef.current?.click()}
            onDrop={handleDrop}
            onDragOver={(e) => e.preventDefault()}
          >
            <ImagePlus size={48} className="text-muted-foreground" />
            <div className="text-center">
              <p className="font-medium">Drop a photo here</p>
              <p className="text-sm text-muted-foreground">or click to browse</p>
            </div>
            <Button variant="outline" size="sm" type="button">Select from device</Button>
          </div>
        ) : (
          <div className="relative">
            <Image
              src={preview}
              alt="Preview"
              width={640}
              height={640}
              className="max-h-96 w-full object-contain bg-muted"
              unoptimized
            />
            <button
              onClick={() => { setPreview(null); setFile(null); }}
              className="absolute right-2 top-2 rounded-full bg-black/60 p-1.5 text-white hover:bg-black/80"
              aria-label="Remove image"
            >
              <X size={16} />
            </button>
          </div>
        )}

        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={handleFileChange}
        />

        {/* Caption + submit */}
        <div className="space-y-3 p-4">
          {error && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          <div className="flex items-start gap-3">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold uppercase">
              {user?.username[0]}
            </div>
            <textarea
              value={caption}
              onChange={(e) => setCaption(e.target.value)}
              placeholder="Write a caption…"
              maxLength={2000}
              rows={3}
              className="flex-1 resize-none bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            />
          </div>

          <div className="flex items-center justify-between">
            <span className="text-xs text-muted-foreground">{caption.length}/2000</span>
            <Button
              onClick={handleSubmit}
              disabled={!file || uploading}
              size="sm"
            >
              {uploading ? (
                <><Loader2 size={14} className="mr-2 animate-spin" />Uploading…</>
              ) : (
                "Share"
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}