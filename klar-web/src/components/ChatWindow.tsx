"use client";

import { useEffect, useState, useRef } from "react";
import { chatsApi, ChatMessage } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Reply, Edit2, Trash2, X, Smile } from "lucide-react";
import { getMediaUrl } from "@/lib/utils/media";

interface ChatWindowProps {
  conversationId?: string;
  receiverId: string;
  receiverUsername: string;
  receiverAvatar: string | null;
}

export default function ChatWindow({ conversationId, receiverId, receiverUsername, receiverAvatar }: ChatWindowProps) {
  const { user } = useAuth();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [inputText, setInputText] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // States für die neuen Features
  const [replyingTo, setReplyingTo] = useState<ChatMessage | null>(null);
  const [editingMessage, setEditingMessage] = useState<ChatMessage | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!conversationId || conversationId === "new") return;
    chatsApi.getMessages(conversationId)
      .then(setMessages)
      .catch(err => console.error("Could not load messages:", err));
  }, [conversationId]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim() || !user) return;

    setLoading(true);
    setError(null);
    try {
      if (editingMessage) {
        await chatsApi.editMessage(editingMessage.id, inputText);
        setMessages(prev => prev.map(m => m.id === editingMessage.id ? { ...m, body: inputText, edited_at: new Date().toISOString() } : m));
        setEditingMessage(null);
      } else {
        const newMsg = await chatsApi.sendMessage(receiverId, inputText, replyingTo?.id);
        setMessages(prev => [...prev, newMsg]);
        setReplyingTo(null);
      }
      setInputText("");
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
      setTimeout(() => inputRef.current?.focus(), 10);
    }
  };

  const handleDelete = async (messageId: string) => {
    try {
      await chatsApi.deleteMessage(messageId);
      setMessages(prev => prev.filter(m => m.id !== messageId));
    } catch (err) {
      console.error(err);
    }
  };

  const handleReaction = async (messageId: string, emoji: string) => {
    if (!user) return;
    try {
      await chatsApi.toggleReaction(messageId, emoji);
      // Optimistisches Update der UI
      setMessages(prev => prev.map(m => {
        if (m.id !== messageId) return m;
        const hasReacted = m.reactions.some(r => r.user_id === user.id && r.emoji === emoji);
        let newReactions = [...m.reactions];
        if (hasReacted) {
          newReactions = newReactions.filter(r => !(r.user_id === user.id && r.emoji === emoji));
        } else {
          newReactions.push({ emoji, user_id: user.id, username: user.username });
        }
        return { ...m, reactions: newReactions };
      }));
    } catch (err) {
      console.error(err);
    }
  };

return (
    <div className="flex flex-col h-full w-full bg-background overflow-hidden relative">
      
      {/* Header */}
      <div className="h-20 flex-none p-4 border-b flex items-center gap-3 bg-background/95 backdrop-blur">
        <div className="w-10 h-10 bg-muted rounded-full flex-shrink-0 flex items-center justify-center overflow-hidden">
          {receiverAvatar ? (
            <img src={getMediaUrl(receiverAvatar)} alt={receiverUsername} className="w-full h-full object-cover" />
          ) : (
            <span className="font-bold text-muted-foreground">{receiverUsername.charAt(0).toUpperCase()}</span>
          )}
        </div>
        <div>
          <span className="font-semibold">{receiverUsername}</span>
          <div className="text-xs text-muted-foreground">End-to-End Encrypted</div>
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-6">
        {messages.map((msg) => {
          const isMe = msg.sender_id === user?.id;
          const repliedMsg = msg.reply_to_message_id ? messages.find(m => m.id === msg.reply_to_message_id) : null;

          return (
            <div key={msg.id} className={`flex flex-col group ${isMe ? "items-end" : "items-start"}`}>
              
              {/* Reply Snippet */}
              {repliedMsg && (
                <div className="text-xs text-muted-foreground bg-muted p-2 rounded-t-lg mb-[-10px] pb-3 px-3 max-w-[70%] truncate border">
                  Replying to: {repliedMsg.body}
                </div>
              )}

              <div className="flex items-center gap-2">
                {/* Hover Actions (Left for me, Right for them) */}
                {isMe && (
                  <div className="opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
                    <button onClick={() => setEditingMessage(msg)} className="p-1 hover:bg-muted rounded text-muted-foreground"><Edit2 size={14} /></button>
                    <button onClick={() => handleDelete(msg.id)} className="p-1 hover:bg-destructive/10 rounded text-destructive"><Trash2 size={14} /></button>
                  </div>
                )}

                {/* Message Bubble */}
                <div className={`relative max-w-xs p-3 rounded-2xl ${
                  isMe ? "bg-primary text-primary-foreground rounded-br-sm" : "bg-muted rounded-bl-sm"
                }`}>
                  <p className="break-words">{msg.body}</p>
                  {msg.edited_at && <span className="text-[10px] opacity-70 mt-1 block">(bearbeitet)</span>}
                </div>

                {!isMe && (
                  <div className="opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
                    <button onClick={() => setReplyingTo(msg)} className="p-1 hover:bg-muted rounded text-muted-foreground"><Reply size={14} /></button>
                    <button onClick={() => handleReaction(msg.id, "❤️")} className="p-1 hover:bg-muted rounded text-muted-foreground"><Smile size={14} /></button>
                  </div>
                )}
              </div>

              {/* Reactions Array */}
              {msg.reactions && msg.reactions.length > 0 && (
                <div className="flex gap-1 mt-1 z-10">
                  {Array.from(new Set(msg.reactions.map(r => r.emoji))).map(emoji => {
                    const count = msg.reactions.filter(r => r.emoji === emoji).length;
                    const iReacted = msg.reactions.some(r => r.emoji === emoji && r.user_id === user?.id);
                    return (
                      <button 
                        key={emoji}
                        onClick={() => handleReaction(msg.id, emoji)}
                        className={`text-xs px-2 py-0.5 rounded-full border ${iReacted ? "bg-primary/20 border-primary/50" : "bg-background border-border"}`}
                      >
                        {emoji} {count > 1 && count}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
        <div ref={messagesEndRef} />
      </div>

      {/* Input Area */}
      <div className="flex-none p-3 border-t bg-muted/10">
        {error && <p className="text-sm text-destructive mb-2 px-2">{error}</p>}
        
        {/* Active Reply/Edit Banners */}
        {replyingTo && (
          <div className="flex justify-between items-center bg-muted p-2 rounded-t-md text-sm border-x border-t">
            <span className="truncate text-muted-foreground">Replying to: {replyingTo.body}</span>
            <button onClick={() => setReplyingTo(null)} className="text-muted-foreground hover:text-foreground"><X size={14}/></button>
          </div>
        )}
        {editingMessage && (
          <div className="flex justify-between items-center bg-primary/10 p-2 rounded-t-md text-sm border-x border-t">
            <span className="truncate text-primary">Editing message...</span>
            <button onClick={() => { setEditingMessage(null); setInputText(""); }} className="text-primary hover:text-primary/70"><X size={14}/></button>
          </div>
        )}

        <form onSubmit={handleSubmit} className="flex gap-2">
          <Input 
            ref={inputRef}
            value={inputText}
            onChange={(e) => setInputText(e.target.value)}
            placeholder={editingMessage ? "Edit message..." : `Message @${receiverUsername}...`}
            disabled={loading}
            className={`flex-1 ${replyingTo || editingMessage ? 'rounded-tl-none rounded-tr-none' : ''}`}
            autoFocus
          />
          <Button type="submit" disabled={loading || !inputText.trim()}>
            {editingMessage ? "Save" : "Send"}
          </Button>
        </form>
      </div>
    </div>
  );
}