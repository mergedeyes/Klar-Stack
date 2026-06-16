"use client";

import { useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { chatsApi, type Conversation } from "@/lib/api";
import { ArrowLeft, MessageSquarePlus, MessageCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { getMediaUrl } from "@/lib/utils/media";
import ChatWindow from "@/components/ChatWindow";
import NewChatModal from "@/components/NewChatModal";

export default function UnifiedChatsPage() {
  const router = useRouter();
  const searchParams = useSearchParams();
  
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showNewChat, setShowNewChat] = useState(false);

  // Aktiver Chat (entweder aus bestehenden Conversations oder via URL-Params von der Profilseite)
  const [activeChat, setActiveChat] = useState<{
    id?: string;
    uid: string;
    un: string;
    av: string | null;
  } | null>(null);

  useEffect(() => {
    chatsApi.getConversations()
      .then((data) => {
        setConversations(data);
        
        // Prüfen, ob wir von einem Profil kommen (?uid=...)
        const queryUid = searchParams.get("uid");
        const queryUn = searchParams.get("un");
        const queryAv = searchParams.get("av");

        if (queryUid && queryUn) {
          // Gucken, ob wir den Chat schon in der Liste haben
          const existing = data.find(c => c.other_user_id === queryUid);
          setActiveChat({
            id: existing?.id,
            uid: queryUid,
            un: queryUn,
            av: queryAv || null
          });
        }
      })
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false));
  }, [searchParams]);

  const selectConversation = (conv: Conversation) => {
    setActiveChat({
      id: conv.id,
      uid: conv.other_user_id,
      un: conv.other_username,
      av: conv.other_avatar_url
    });
    // URL säubern ohne neuladen
    window.history.replaceState({}, '', '/chats');
  };

return (
    // 1. Root: h-screen und overflow-hidden zwingen die App, NICHT zu scrollen.
    <div className="h-screen w-full flex flex-col bg-background overflow-hidden">
      
      {/* 2. Page Header: flex-none hält ihn bei exakt h-14. KEIN fixed oder sticky mehr! */}
      <header className="flex-none h-14 border-b border-border bg-background/95 backdrop-blur px-[50px] flex items-center">
        <Button variant="ghost" className="gap-2" onClick={() => router.push("/feed")}>
          <ArrowLeft size={18} />
          Zurück zum Feed
        </Button>
      </header>

      {/* 3. Main: flex-1 nimmt sich den restlichen Platz. overflow-hidden verhindert Scrollen. */}
      <main className="flex-1 w-full flex overflow-hidden px-[50px]">
        
        {/* Linke Sidebar: Chat-Liste */}
        <div className="w-full md:w-1/3 border-r border-border flex flex-col bg-muted/10">
          <div className="h-20 p-4 border-b border-border flex items-center justify-between bg-background">
            <h1 className="text-xl font-bold">Chats</h1>
            <Button variant="ghost" size="icon" onClick={() => setShowNewChat(true)}>
              <MessageSquarePlus size={20} />
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-1">
            {loading && <div className="p-4 text-center text-muted-foreground animate-pulse">Lade Chats...</div>}
            {error && <div className="p-4 text-center text-destructive">{error}</div>}
            
            {!loading && conversations.length === 0 && (
              <div className="p-8 text-center text-muted-foreground text-sm">
                Keine aktiven Chats.
              </div>
            )}

            {conversations.map((conv) => (
              <button
                key={conv.id}
                onClick={() => selectConversation(conv)}
                className={`w-full flex items-center p-3 rounded-xl transition-colors text-left ${
                  activeChat?.id === conv.id ? "bg-primary/10" : "hover:bg-muted"
                }`}
              >
                <div className="w-12 h-12 bg-background border rounded-full flex-shrink-0 mr-4 flex items-center justify-center overflow-hidden">
                  {conv.other_avatar_url ? (
                    <img src={getMediaUrl(conv.other_avatar_url)} alt={conv.other_username} className="w-full h-full object-cover" />
                  ) : (
                    <span className="font-bold">{conv.other_username.charAt(0).toUpperCase()}</span>
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  <h2 className="font-semibold truncate">{conv.other_username}</h2>
                  <p className="text-sm text-muted-foreground truncate">
                    {conv.last_message || "Neuer Chat"}
                  </p>
                </div>
              </button>
            ))}
          </div>
        </div>

        {/* Rechte Hauptfläche: Chat-Fenster */}
        <div className="hidden md:flex flex-1 flex-col bg-background border-r border-border overflow-hidden">
          {activeChat ? (
            <ChatWindow 
              // Ein key erzwingt einen kompletten Rerender des ChatWindows, wenn der User wechselt
              key={activeChat.uid} 
              conversationId={activeChat.id}
              receiverId={activeChat.uid}
              receiverUsername={activeChat.un}
              receiverAvatar={activeChat.av}
            />
          ) : (
            <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground">
              <MessageCircle size={48} className="mb-4 opacity-20" />
              <p>Wähle einen Chat aus der Liste oder starte einen neuen.</p>
            </div>
          )}
        </div>
      </main>

      {/* Modal */}
      {showNewChat && (
        <NewChatModal 
          onClose={() => setShowNewChat(false)} 
          existingConversations={conversations} 
        />
      )}
    </div>
  );
}