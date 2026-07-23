"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { chatsApi, type Conversation, type ChatMessage } from "@/lib/api";
import { MessageSquarePlus, MessageCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { getMediaUrl } from "@/lib/utils/media";
import { useAuth } from "@/lib/auth-context";
import { useNotifications } from "@/hooks/use-notifications";
import ChatWindow from "@/components/ChatWindow";
import NewChatModal from "@/components/NewChatModal";
import TopNav from "@/components/TopNav";

/** Sort newest-updated first -- same ordering the backend already
 * returns, but re-applied client-side after a local, optimistic update
 * (sending or receiving a message) moves a conversation to the top. */
function sortByRecency(convos: Conversation[]): Conversation[] {
  return [...convos].sort(
    (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
  );
}

/** "Me: <message>" if the current user sent it, otherwise just the
 * message text -- no "<username>: " prefix for the other person. */
function previewText(conv: Conversation, currentUserId: string | undefined): string {
  if (!conv.last_message) return "Neuer Chat";
  return conv.last_message_sender_id === currentUserId
    ? `Me: ${conv.last_message}`
    : conv.last_message;
}

function UnifiedChatsPageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const { user } = useAuth();
  const { lastMessageEvent, refreshChatUnreadCount } = useNotifications();

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
          // Coming from a profile link straight into an existing
          // conversation counts as opening it too.
          if (existing?.id) {
            chatsApi.markConversationRead(existing.id).catch(err =>
              console.error("Failed to mark conversation as read", err)
            );
            refreshChatUnreadCount();
          }
        }
      })
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams]);

  // A live "message" event arrived (see hooks/use-notifications.ts) --
  // refetch the conversation list so its preview text/ordering reflects
  // it instantly, rather than only on the next page load. A full refetch
  // (vs. trying to patch just one row) also correctly handles a message
  // arriving in a conversation that isn't in the list yet.
  useEffect(() => {
    if (!lastMessageEvent) return;
    chatsApi.getConversations()
      .then(setConversations)
      .catch(err => console.error("Failed to refresh conversations:", err));
  }, [lastMessageEvent]);

  const selectConversation = (conv: Conversation) => {
    setActiveChat({
      id: conv.id,
      uid: conv.other_user_id,
      un: conv.other_username,
      av: conv.other_avatar_url
    });
    // Opening a conversation clears its unread messages. Fire-and-forget
    // the mark-read call, but refresh the badge count immediately after —
    // without this, the red dot on the Chat icon only cleared on the next
    // full mount of the notification provider (e.g. a reload), since
    // nothing else told it a conversation had just been read.
    chatsApi.markConversationRead(conv.id).catch(err =>
      console.error("Failed to mark conversation as read", err)
    );
    refreshChatUnreadCount();
    // URL säubern ohne neuladen
    window.history.replaceState({}, '', '/chats');
  };

  // Called by ChatWindow right after *this* user successfully sends a
  // message -- updates that conversation's preview instantly instead of
  // waiting for anything else to trigger a refetch.
  const handleMessageSent = (message: ChatMessage) => {
    setConversations((prev) => {
      const updated = prev.map((c) =>
        c.id === message.conversation_id
          ? {
              ...c,
              last_message: message.body,
              last_message_sender_id: message.sender_id,
              updated_at: message.created_at,
            }
          : c
      );
      return sortByRecency(updated);
    });
  };

return (
    // 1. Root: h-screen und overflow-hidden zwingen die App, NICHT zu scrollen.
    <div className="h-screen w-full flex flex-col bg-background overflow-hidden">

      {/* 2. Shared top nav, same as every other primary page -- also gives
         this page its own SSE-backed notification state (via the
         NotificationsProvider mounted in layout.tsx), which is what
         ChatWindow relies on for live message updates. */}
      <TopNav active="chats" />

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
                    {previewText(conv, user?.id)}
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
              onMessageSent={handleMessageSent}
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

export default function UnifiedChatsPage() {
  return (
    <Suspense
      fallback={
        <div className="flex h-screen w-full items-center justify-center bg-background text-muted-foreground">
          Loading chats…
        </div>
      }
    >
      <UnifiedChatsPageContent />
    </Suspense>
  );
}
