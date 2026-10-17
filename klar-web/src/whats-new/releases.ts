// "What's new": what changed for users, newest first. The first entry is
// shown once over the feed to every account that signed up before its day
// and hasn't seen it (components/WhatsNew.tsx); /whats-new lists them all.
// An entry goes in when a set of changes is worth telling people about,
// not with every pull request.
//
// The id is the release date, "YYYY-MM-DD", with "-b", "-c", ... for a
// second one on the same day: ids are compared as text, so they must sort
// by date (the backend checks the shape, validation.rs validate_release_id).

export interface ReleaseItem {
  title: string;
  text: string;
  /** Where to try it, e.g. a settings page. */
  href?: string;
}

export interface ReleaseText {
  items: ReleaseItem[];
}

export interface Release {
  id: string;
  en: ReleaseText;
  de: ReleaseText;
}

export const RELEASES: Release[] = [
  {
    id: "2026-10-18",
    en: {
      items: [
        {
          title: "Klar on your home screen, with your icon",
          text: "Add Klar to your home screen and it opens like an app, on your feed. Pick its icon from Klar's new logos under Settings → Language and app icon.",
          href: "/settings/appearance",
        },
        {
          title: "Klar speaks German and English",
          text: "Klar follows your browser's language. You can change it under Settings → Language and app icon.",
          href: "/settings/appearance",
        },
        {
          title: "Sign in with your username",
          text: "The login takes your username as well as your email address.",
        },
        {
          title: "New posts at a glance",
          text: "When new posts come in while you're reading, a button at the top says how many. Nothing jumps around until you tap it.",
        },
        {
          title: "Nothing missed in the background",
          text: "Messages and notifications that arrived while your phone was locked show up as soon as you're back.",
        },
      ],
    },
    de: {
      items: [
        {
          title: "Klar auf deinem Startbildschirm, mit deinem Symbol",
          text: "Füg Klar zu deinem Startbildschirm hinzu, dann öffnet es sich wie eine App, direkt in deinem Feed. Das Symbol wählst du aus Klars neuen Logos unter Einstellungen → Sprache und App-Symbol.",
          href: "/settings/appearance",
        },
        {
          title: "Klar spricht Deutsch und Englisch",
          text: "Klar richtet sich nach der Sprache deines Browsers. Ändern kannst du sie unter Einstellungen → Sprache und App-Symbol.",
          href: "/settings/appearance",
        },
        {
          title: "Anmelden mit Benutzernamen",
          text: "Beim Anmelden geht jetzt auch dein Benutzername statt der E-Mail-Adresse.",
        },
        {
          title: "Neue Beiträge auf einen Blick",
          text: "Kommen beim Lesen neue Beiträge dazu, zeigt dir oben ein Button, wie viele. Es verrutscht nichts, bis du ihn antippst.",
        },
        {
          title: "Nichts verpasst im Hintergrund",
          text: "Nachrichten und Benachrichtigungen, die kamen, während dein Handy gesperrt war, erscheinen, sobald du zurück bist.",
        },
      ],
    },
  },
];

/** The release to show this account, if any: the newest, when it's newer
 * than what the account has seen (or than its sign-up day). */
export function releaseToShow(seen: string | undefined): Release | undefined {
  const latest = RELEASES[0];
  if (!latest || seen === undefined) return undefined;
  return latest.id > seen ? latest : undefined;
}

/** The day a release came out, for display. */
export function releaseDate(release: Release, locale: string): string {
  return new Date(`${release.id.slice(0, 10)}T12:00:00Z`).toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}
