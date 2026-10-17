import type en from "../../messages/en.json";
import de from "../../messages/de.json";

// Never imported: it only makes `tsc` fail when the German file lacks a
// message the English one has, which would otherwise show up as a raw key
// on the page.
export const germanHasEveryMessage: typeof en = de;
