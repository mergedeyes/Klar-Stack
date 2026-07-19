import { X } from "lucide-react";
import Image from "next/image";
import Link from "next/link";
import type { User } from "@/lib/api";
import { getMediaUrl } from "@/lib/utils/media";
import { ENV } from '../env';

const API_URL = ENV.API_URL;

interface UserListModalProps {
  title: string;
  users: User[];
  onClose: () => void;
}

export default function UserListModal({ title, users, onClose }: UserListModalProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
      <div className="flex max-h-[80vh] w-full max-w-sm flex-col overflow-hidden rounded-xl border border-border bg-background shadow-lg">
        
        {/* Header */}
        <div className="flex items-center justify-between border-b border-border p-4">
          <h2 className="text-lg font-semibold">{title}</h2>
          <button
            onClick={onClose}
            className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <X size={20} />
          </button>
        </div>

        {/* User List */}
        <div className="flex-1 overflow-y-auto p-2">
          {users.length === 0 ? (
            <div className="p-8 text-center text-sm text-muted-foreground">
              No users found.
            </div>
          ) : (
            users.map((user) => (
              <Link
                key={user.id}
                href={`/users/${user.username}`}
                onClick={onClose}
                className="flex items-center gap-3 rounded-lg p-2 transition-colors hover:bg-muted"
              >
                <div className="relative h-10 w-10 shrink-0 overflow-hidden rounded-full bg-muted">
                  {user.avatar_url ? (
                    <Image
                      src={getMediaUrl(user.avatar_url)}
                      alt={user.username}
                      fill
                      className="object-cover"
                      unoptimized
                    />
                  ) : (
                    <span className="flex h-full w-full items-center justify-center text-sm font-bold uppercase">
                      {user.username[0]}
                    </span>
                  )}
                </div>
                <div className="flex flex-col">
                  <span className="text-sm font-semibold">{user.username}</span>
                  {user.display_name && (
                    <span className="text-xs text-muted-foreground">
                      {user.display_name}
                    </span>
                  )}
                </div>
              </Link>
            ))
          )}
        </div>
      </div>
    </div>
  );
}