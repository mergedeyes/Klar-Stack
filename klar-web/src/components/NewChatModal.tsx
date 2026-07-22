"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { User, follows, type Conversation } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { X, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getMediaUrl } from "@/lib/utils/media";

interface NewChatModalProps {
  onClose: () => void;
  existingConversations: Conversation[];
}

export default function NewChatModal({ onClose, existingConversations }: NewChatModalProps) {
  const { user } = useAuth();
  const router = useRouter();
  const [mutuals, setMutuals] = useState<User[]>([]);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!user) return;
    
    // Holt Followers und Following parallel und filtert die Schnittmenge
    Promise.all([
      follows.followers(user.username),
      follows.following(user.username)
    ]).then(([followers, following]) => {
      const mutualUsers = followers.filter(f => 
        following.some(fw => fw.id === f.id)
      );
      setMutuals(mutualUsers);
      setLoading(false);
    }).catch(err => {
      console.error("Fehler beim Laden der Mutuals:", err);
      setLoading(false);
    });
  }, [user]);

  const handleStartChat = (selectedUser: User) => {
    // /chats reads all state from query params (?uid=&un=&av=) — it has no
    // dynamic [id]/[new] route at all, so navigating to /chats/<id> or
    // /chats/new (path segments) 404s. The page itself already looks up
    // whether a conversation exists by matching `uid` against the fetched
    // conversation list, so there's nothing an existing conversation's id
    // needs to add here — same query string works for both cases.
    const queryParams = `?uid=${selectedUser.id}&un=${encodeURIComponent(selectedUser.username)}&av=${encodeURIComponent(selectedUser.avatar_url || '')}`;

    router.push(`/chats${queryParams}`);
    onClose();
  };

  const filteredMutuals = mutuals.filter(m => 
    m.username.toLowerCase().includes(search.toLowerCase()) || 
    m.display_name?.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-card border rounded-xl shadow-lg flex flex-col max-h-[80vh]">
        
        <div className="flex items-center justify-between p-4 border-b">
          <h2 className="text-lg font-bold">Neuer Chat</h2>
          <Button variant="ghost" size="icon" onClick={onClose}>
            <X size={20} />
          </Button>
        </div>

        <div className="p-4 border-b">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" size={16} />
            <Input 
              placeholder="Suche nach Freunden..." 
              className="pl-9"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2">
          {loading ? (
            <div className="p-8 text-center text-muted-foreground">Lade Freunde...</div>
          ) : mutuals.length === 0 ? (
            <div className="p-8 text-center text-muted-foreground">
              Du hast noch keine gegenseitigen Follower. <br/>Folge anderen, damit sie dir zurückfolgen können!
            </div>
          ) : filteredMutuals.length === 0 ? (
            <div className="p-8 text-center text-muted-foreground">Keine User gefunden.</div>
          ) : (
            filteredMutuals.map(m => (
              <button 
                key={m.id}
                onClick={() => handleStartChat(m)}
                className="w-full flex items-center p-3 hover:bg-muted/50 rounded-lg transition-colors text-left"
              >
                <div className="w-10 h-10 bg-muted rounded-full flex items-center justify-center mr-3 overflow-hidden">
                  {m.avatar_url ? (
                    <img src={getMediaUrl(m.avatar_url)} alt={m.username} className="w-full h-full object-cover" />
                  ) : (
                    <span className="font-bold">{m.username.charAt(0).toUpperCase()}</span>
                  )}
                </div>
                <div>
                  <div className="font-semibold">{m.display_name || m.username}</div>
                  <div className="text-xs text-muted-foreground">@{m.username}</div>
                </div>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
