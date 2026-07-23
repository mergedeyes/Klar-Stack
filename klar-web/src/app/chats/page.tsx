"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import Link from "next/link";
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
 * (sending a message) moves a conversation to the top. */
function sortByRecency(convos: Conversation[]): Conversation[] {
  return [...convos].sort(
    (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
  );
}

/** Renders the conversation-list preview: "Me: <message>" / "<message>"
 * for plain messages and replies, or "<name> reacted 👍 to your message:
 * <text>" for the most recent activity being a reaction -- whichever of
 * the two actually happened more recently (last_activity_kind already
 * reflects that; see get_conversations). */
function previewText(conv: Conversation, currentUserId: string | undefined): string {
  if (!conv.last_activity_kind) return "Neuer Chat";

  const actorIsMe = conv.last_activity_actor_id === currentUserId;
  const actorLabel = actorIsMe ? "Me" : conv.other_username;
  const text = conv.last_activity_text ?? "";

  if (conv.last_activity_kind === "reaction") {
    const messageIsMine = conv.last_activity_message_sender_id === currentUserId;
    const possessive = messageIsMine ? "your" : `${conv.other_username}'s`;
    const emoji = conv.last_activity_emoji ? `${conv.last_activity_emoji} ` : "";
    return `${actorLabel} reacted ${emoji}to ${possessive} message: ${text}`;
  }

  if (conv.last_activity_kind === "reply") {
    return actorIsMe ? `Me: replied: ${text}` : `${conv.other_username} replied: ${text}`;
  }

  return actorIsMe ? `Me: ${text}` : text;
}

/** Mark a conversation read, then refresh the shared unread-count badge --
 * chained rather than fired in parallel, since firing them together let
 * the count refresh race ahead of the mark-read PATCH actually committing,
 * which is why the red dot sometimes didn't clear. */
function markReadAndRefresh(conversationId: string, refresh: () => void) {
  chatsApi.markConversationRead(conversationId)
    .then(() => refresh())
    .catch(err => console.error("Failed to mark conversation as read", err));
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
            markReadAndRefresh(existing.id, refreshChatUnreadCount);
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
  // or reaction arriving in a conversation that isn't in the list yet.
  // This fires for reactions too (chats.rs's toggle_reaction publishes
  // the same event type), which is exactly what's wanted here -- the
  // richer preview needs to reflect a reaction as "last activity" too.
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
    // Opening a conversation clears its unread messages.
    markReadAndRefresh(conv.id, refreshChatUnreadCount);
    // URL säubern ohne neuladen
    window.history.replaceState({}, '', '/chats');
  };

  // Called by ChatWindow right after *this* user successfully sends a
  // message -- updates that conversation's preview instantly instead of
  // waiting for anything else to trigger a refetch. Reactions aren't
  // covered here (they don't go through ChatWindow's send path); those
  // rely on the lastMessageEvent-triggered refetch above instead.
  const handleMessageSent = (message: ChatMessage) => {
    setConversations((prev) => {
      const updated = prev.map((c) =>
        c.id === message.conversation_id
          ? {
              ...c,
              last_activity_kind: (message.reply_to_message_id ? "reply" : "message") as Conversation["last_activity_kind"],
              last_activity_actor_id: message.sender_id,
              last_activity_message_sender_id: message.sender_id,
              last_activity_text: message.body,
              last_activity_emoji: null,
              updated_at: message.created_at,
            }
          : c
      );
      return sortByRecency(updated);
    });
  };

return (
    // 1. Root: h-dvh (dynamic viewport height) instead of h-screen (100vh)
    // -- 100vh doesn't account for mobile browser chrome (address bar
    // etc.), so the page thought it had more room than was actually
    // visible, pushing content (including TopNav) below the fold and
    // forcing a scroll to see it. h-dvh tracks the real visible viewport.
    <div className="h-dvh w-full flex flex-col bg-background overflow-hidden">

      {/* 2. Shared top nav, wrapped in flex-none so it can never be
         compressed by flex-shrink if content below overflows -- without
         this, a flex-col container's default flex-shrink:1 on every
         child (including the header) means overflow pressure from main
         could squeeze the nav instead of just scrolling/clipping main. */}
      <div className="flex-none">
        <TopNav active="chats" />
      </div>

      {/* 3. Main: flex-1 nimmt sich den restlichen Platz. overflow-hidden verhindert Scrollen. */}
      <main className="flex-1 w-full flex overflow-hidden px-[50px] min-h-0">
        
        {/* Linke Sidebar: Chat-Liste */}
        <div className="w-full md:w-1/3 border-r border-border flex flex-col bg-muted/10 min-h-0">
          <div className="h-20 p-4 border-b border-border flex items-center justify-between bg-background">
            <h1 className="text-xl font-bold">Chats</h1>
            <Button variant="ghost" size="icon" onClick={() => setShowNewChat(true)}>
              <MessageSquarePlus size={20} />
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-1 min-h-0">
            {loading && <div className="p-4 text-center text-muted-foreground animate-pulse">Lade Chats...</div>}
            {error && <div className="p-4 text-center text-destructive">{error}</div>}
            
            {!loading && conversations.length === 0 && (
              <div className="p-8 text-center text-muted-foreground text-sm">
                Keine aktiven Chats.
              </div>
            )}

            {conversations.map((conv) => (
              // A plain div (not a button) here on purpose: the avatar is
              // its own Link to the profile, and nesting a link inside a
              // button isn't valid HTML. Clicking the avatar goes to the
              // profile (stopPropagation keeps the row click from also
              // firing); clicking anywhere else in the row opens the chat.
              <div
                key={conv.id}
                role="button"
                tabIndex={0}
                onClick={() => selectConversation(conv)}
                onKeyDown={(e) => { if (e.key === "Enter") selectConversation(conv); }}
                className={`w-full flex items-center p-3 rounded-xl transition-colors text-left cursor-pointer ${
                  activeChat?.id === conv.id ? "bg-primary/10" : "hover:bg-muted"
                }`}
              >
                <Link
                  href={`/users/${conv.other_username}`}
                  onClick={(e) => e.stopPropagation()}
                  className="w-12 h-12 bg-background border rounded-full flex-shrink-0 mr-4 flex items-center justify-center overflow-hidden"
                >
                  {conv.other_avatar_url ? (
                    <img src={getMediaUrl(conv.other_avatar_url)} alt={conv.other_username} className="w-full h-full object-cover" />
                  ) : (
                    <span className="font-bold">{conv.other_username.charAt(0).toUpperCase()}</span>
                  )}
                </Link>
                <div className="flex-1 min-w-0">
                  <h2 className="font-semibold truncate">{conv.other_username}</h2>
                  <p className="text-sm text-muted-foreground truncate">
                    {previewText(conv, user?.id)}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Rechte Hauptfläche: Chat-Fenster */}
        <div className="hidden md:flex flex-1 flex-col bg-background border-r border-border overflow-hidden min-h-0">
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
        <div className="flex h-dvh w-full items-center justify-center bg-background text-muted-foreground">
          Loading chats…
        </div>
      }
    >
      <UnifiedChatsPageContent />
    </Suspense>
  );
}
