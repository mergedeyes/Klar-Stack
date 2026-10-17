"use client";

import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { usePathname } from "next/navigation";
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

function steps(emailVerified: boolean): Step[] {
  return [
    {
      icon: Home,
      title: "Welcome to Klar",
      body: (
        <>
          <p>
            Your home feed shows posts from the people you follow, newest first. Nothing is sorted by likes, and
            there are no ads.
          </p>
          <p>Here&apos;s a quick look around. It takes a minute.</p>
        </>
      ),
    },
    {
      icon: Search,
      title: "Find people",
      body: (
        <>
          <p>
            Tap <Icon icon={Search} label="Search" /> at the top to look people up by username or name, then
            press <strong>Follow</strong> on their profile.
          </p>
          <p>
            Private accounts approve each follower first. Until they do, the button says{" "}
            <strong>Requested</strong>.
          </p>
          <p>
            <Icon icon={Compass} label="Discovery" /> opens Discovery, the newest public posts from everyone. It&apos;s
            a good place to find people to follow.
          </p>
        </>
      ),
    },
    {
      icon: Plus,
      title: "Share and react",
      body: (
        <>
          <p>
            Tap <Icon icon={Plus} label="New post" /> to share photos with a caption. Anyone can see the posts of a
            public account; only followers see those of a private one.
          </p>
          <p>
            Open a post to like and comment. <Icon icon={Bell} label="Notifications" /> shows who followed you,
            liked or commented, and lets you accept or decline follow requests.
          </p>
          {!emailVerified && (
            <p className="text-muted-foreground">
              Posting, commenting and messaging start once you&apos;ve clicked the link in the email we sent you.
            </p>
          )}
        </>
      ),
    },
    {
      icon: MessageCircle,
      title: "Chats",
      body: (
        <>
          <p>
            You can chat with people who follow you back. Open <Icon icon={MessageCircle} label="Chats" /> and
            start a new chat with one of them.
          </p>
          <p>Reply to a message, react to it with an emoji, or edit and delete your own.</p>
          <p className="text-muted-foreground">
            Chats aren&apos;t end-to-end encrypted, so don&apos;t share anything there that must stay strictly
            secret.
          </p>
        </>
      ),
    },
    {
      icon: Flag,
      title: "Staying safe",
      body: (
        <>
          <p>
            Report a post or profile with <Icon icon={Flag} label="Report" />, and a comment with the{" "}
            <strong>Report</strong> link under it. We review every report and let you know the outcome.
          </p>
          <p>
            Press <strong>Block</strong> on a profile, and neither of you can follow, message, like or comment on
            the other any more. You can undo it in Settings → Blocked accounts.
          </p>
        </>
      ),
    },
    {
      icon: Settings,
      title: "Your choices",
      body: (
        <>
          <p>Klar is still in a test phase, so one choice comes first:</p>
          <div className="-mx-1 overflow-hidden rounded-xl border border-border">
            <KeepAccountRow />
          </div>
          <p>
            You can change it later in <Icon icon={Settings} label="Settings" />, where you can also make your
            account private, download your data, and turn on personalised Discovery, which stays off unless you
            choose it.
          </p>
          <p>
            Found a bug or have an idea? Use <strong>Feedback</strong> at the bottom of the page. This guide is
            there too.
          </p>
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
  const all = steps(emailVerified);
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
          aria-label="Close the guide"
        >
          <X size={18} />
        </button>

        <span className="mb-3 flex h-10 w-10 items-center justify-center rounded-full border border-border">
          <step.icon size={20} />
        </span>
        <p className="mb-1 text-xs text-muted-foreground">
          Step {index + 1} of {all.length}
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
              Back
            </Button>
          ) : (
            <Button variant="ghost" onClick={onClose}>
              Skip
            </Button>
          )}
          <Button ref={nextRef} className="flex-1" onClick={() => (last ? onClose() : setIndex(index + 1))}>
            {last ? "Let's go" : "Next"}
          </Button>
        </div>
      </div>
    </div>
  );
}
