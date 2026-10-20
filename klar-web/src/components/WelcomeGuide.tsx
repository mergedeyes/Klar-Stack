"use client";

import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { usePathname } from "next/navigation";
import { useTranslations } from "next-intl";
import {
  Bell,
  Compass,
  Flag,
  Home,
  MessageCircle,
  Plus,
  Search,
  Settings,
  X,
  type LucideIcon,
} from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { Button } from "@/components/ui/button";
import KeepAccountRow from "@/components/settings/KeepAccountRow";

// A few short steps for a new account, shown once over the feed after the
// first sign-in (users.guide_seen_at) and again whenever the footer link
// opens it. Text only: it names the icons in the top bar instead of
// pointing at them, so it doesn't break when the bar changes.

interface Step {
  icon: LucideIcon;
  title: string;
  body: ReactNode;
}

/** An icon as it appears in the top bar, inline in a sentence. */
function Icon({ icon: I, label }: { icon: LucideIcon; label: string }) {
  return (
    <span className="mx-0.5 inline-flex h-5 w-5 items-center justify-center rounded border border-border align-text-bottom" aria-label={label} role="img">
      <I size={13} />
    </span>
  );
}

function steps(emailVerified: boolean, t: ReturnType<typeof useTranslations<"guide">>, tn: ReturnType<typeof useTranslations<"nav">>, tp: ReturnType<typeof useTranslations<"post">>): Step[] {
  // The top-bar icons and the bold button names inside the sentences.
  const tags = {
    b: (chunks: ReactNode) => <strong>{chunks}</strong>,
    search: () => <Icon icon={Search} label={tn("search")} />,
    discovery: () => <Icon icon={Compass} label={tn("discovery")} />,
    newPost: () => <Icon icon={Plus} label={tn("newPost")} />,
    bell: () => <Icon icon={Bell} label={tn("notifications")} />,
    chats: () => <Icon icon={MessageCircle} label={tn("chats")} />,
    flag: () => <Icon icon={Flag} label={tp("report")} />,
    settings: () => <Icon icon={Settings} label={tn("settings")} />,
  };
  return [
    {
      icon: Home,
      title: t("welcomeTitle"),
      body: (
        <>
          <p>{t("welcome1")}</p>
          <p>{t("welcome2")}</p>
        </>
      ),
    },
    {
      icon: Search,
      title: t("findTitle"),
      body: (
        <>
          <p>{t.rich("find1", tags)}</p>
          <p>{t.rich("find2", tags)}</p>
          <p>{t.rich("find3", tags)}</p>
        </>
      ),
    },
    {
      icon: Plus,
      title: t("shareTitle"),
      body: (
        <>
          <p>{t.rich("share1", tags)}</p>
          <p>{t.rich("share2", tags)}</p>
          {!emailVerified && <p className="text-muted-foreground">{t("shareUnverified")}</p>}
        </>
      ),
    },
    {
      icon: MessageCircle,
      title: t("chatsTitle"),
      body: (
        <>
          <p>{t.rich("chats1", tags)}</p>
          <p>{t("chats2")}</p>
          <p className="text-muted-foreground">{t("chats3")}</p>
        </>
      ),
    },
    {
      icon: Flag,
      title: t("safeTitle"),
      body: (
        <>
          <p>{t.rich("safe1", tags)}</p>
          <p>{t.rich("safe2", tags)}</p>
        </>
      ),
    },
    {
      icon: Settings,
      title: t("choicesTitle"),
      body: (
        <>
          <p>{t("choices1")}</p>
          <div className="-mx-1 overflow-hidden rounded-xl border border-border">
            <KeepAccountRow />
          </div>
          <p>{t.rich("choices2", tags)}</p>
          <p>{t.rich("choices3", tags)}</p>
        </>
      ),
    },
  ];
}

const GuideContext = createContext<{ openGuide: () => void }>({ openGuide: () => {} });

/** Opens the guide from anywhere below the provider (the footer link). */
export function useGuide() {
  return useContext(GuideContext);
}

export function GuideProvider({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const pathname = usePathname();
  const [opened, setOpened] = useState(false);
  // The account that closed it in this tab, so it doesn't come back on the
  // next visit to the feed before GET /users/me says it was seen, but does
  // for another new account signing in here.
  const [closedBy, setClosedBy] = useState<string | null>(null);
  const closed = !!user && closedBy === user.id;

  // Only over the feed, where sign-up and sign-in land, so it never covers
  // the legal pages or a verification link.
  const firstVisit = !!user && user.guide_seen === false && !closed && pathname === "/feed";
  const open = !!user && (opened || firstVisit);

  const close = () => {
    setOpened(false);
    if (!closed && user?.guide_seen === false) {
      // If this fails, the guide comes back next time; nothing worse.
      users.markGuideSeen().catch(() => {});
    }
    if (user) setClosedBy(user.id);
  };

  return (
    <GuideContext value={{ openGuide: () => setOpened(true) }}>
      {children}
      {open && user && <GuideDialog emailVerified={user.email_verified} onClose={close} />}
    </GuideContext>
  );
}

function GuideDialog({ emailVerified, onClose }: { emailVerified: boolean; onClose: () => void }) {
  const [index, setIndex] = useState(0);
  const t = useTranslations("guide");
  const tn = useTranslations("nav");
  const tp = useTranslations("post");
  const tc = useTranslations("common");
  const all = steps(emailVerified, t, tn, tp);
  const step = all[index];
  const last = index === all.length - 1;
  const nextRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    nextRef.current?.focus();
  }, [index]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="guide-title"
        className="relative flex max-h-[90dvh] w-full max-w-md flex-col overflow-y-auto rounded-xl bg-background p-5 shadow-xl"
      >
        <button
          onClick={onClose}
          className="absolute right-3 top-3 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label={t("close")}
        >
          <X size={18} />
        </button>

        <span className="mb-3 flex h-10 w-10 items-center justify-center rounded-full border border-border">
          <step.icon size={20} />
        </span>
        <p className="mb-1 text-xs text-muted-foreground">
          {t("step", { n: index + 1, total: all.length })}
        </p>
        <h2 id="guide-title" className="mb-3 text-lg font-semibold">
          {step.title}
        </h2>
        <div className="mb-5 space-y-3 text-sm leading-relaxed">{step.body}</div>

        <div className="mb-4 flex justify-center gap-1.5" aria-hidden="true">
          {all.map((s, i) => (
            <span key={s.title} className={`h-1.5 w-1.5 rounded-full ${i === index ? "bg-foreground" : "bg-muted-foreground/30"}`} />
          ))}
        </div>

        <div className="flex items-center gap-2">
          {index > 0 ? (
            <Button variant="outline" onClick={() => setIndex(index - 1)}>
              {tc("back")}
            </Button>
          ) : (
            <Button variant="ghost" onClick={onClose}>
              {t("skip")}
            </Button>
          )}
          <Button ref={nextRef} className="flex-1" onClick={() => (last ? onClose() : setIndex(index + 1))}>
            {last ? t("letsGo") : t("next")}
          </Button>
        </div>
      </div>
    </div>
  );
}
