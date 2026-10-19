// Builds Klar's app icons from the branding folder (Dokumente/Klar/Branding/Logo):
//
//   node tools/build-app-icons.mjs <branding/Logo folder>
//
// Reads its icons.json and, per icon, writes public/app-icons/<id>/:
//   192.png, 512.png   the icon as designed, rounded corners and all
//   maskable.png       512px, full-bleed for Android's circle and squircle
//                      masks: scaled up by 16% so the rounded corners fall
//                      outside the square, any remaining transparency white
//   apple.png          180px, full-bleed the same way (iOS rounds it itself
//                      and shows transparency as black)
// plus public/favicon.ico from the default icon, and src/app-icons/catalog.json
// for the picker, with English names next to the designer's German ones.
// Run it again whenever the branding folder changes, and commit the result.

import { mkdirSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import sharp from "sharp";

const source = process.argv[2];
if (!source) {
  console.error("usage: node tools/build-app-icons.mjs <branding/Logo folder>");
  process.exit(1);
}
const web = path.resolve(import.meta.dirname, "..");
const spec = JSON.parse(readFileSync(path.join(source, "icons.json"), "utf8"));

// The designer's names are German; these are the English ones. A name
// missing here stays German in English, with a warning below.
const ENGLISH = {
  Klar: "Klar", Sucher: "Viewfinder", Bausteine: "Blocks", Glas: "Glass", Kunst: "Art", Retro: "Retro",
  Phaser: "Phaser", "80er": "80s", "90er": "90s", Y2K: "Y2K",
  Violett: "Violet", Blau: "Blue", Hell: "Light", Koralle: "Coral", Nacht: "Night", Mono: "Mono", "Grün": "Green",
  Klassik: "Classic", Kristall: "Crystal", Linse: "Lens", Prisma: "Prism",
  Mosaik: "Mosaic", Original: "Original", Rauschen: "Noise", Spiegelung: "Reflection", Sprechblase: "Speech bubble",
  Memphis: "Memphis", Neon: "Neon", Synthwave: "Synthwave", Arcade: "Arcade", Fresh: "Fresh", WordArt: "WordArt",
  Aqua: "Aqua", Chrom: "Chrome", Holo: "Holo",
};
const label = (de) => {
  if (!(de in ENGLISH)) console.warn(`no English name for "${de}"; add it to ENGLISH`);
  return { de, en: ENGLISH[de] ?? de };
};

const BLEED = 1.16;

async function fullBleed(file, size) {
  const big = Math.round(size * BLEED);
  const offset = Math.round((big - size) / 2);
  return sharp(file)
    .resize(big, big)
    .extract({ left: offset, top: offset, width: size, height: size })
    .flatten({ background: "#ffffff" })
    .png({ compressionLevel: 9, effort: 10 })
    .toBuffer();
}

const out = path.join(web, "public/app-icons");
rmSync(out, { recursive: true, force: true });
for (const icon of spec.icons) {
  const src = path.join(source, icon.files.png2x);
  const dir = path.join(out, icon.id);
  mkdirSync(dir, { recursive: true });
  for (const size of [192, 512]) {
    await sharp(src).resize(size, size).png({ compressionLevel: 9, effort: 10 }).toFile(path.join(dir, `${size}.png`));
  }
  writeFileSync(path.join(dir, "maskable.png"), await fullBleed(src, 512));
  writeFileSync(path.join(dir, "apple.png"), await fullBleed(src, 180));
}

// The tab icon for browsers that ask for /favicon.ico, from the default.
const fallback = spec.icons.find((i) => i.id === spec.default);
const tmp = path.join(web, ".favicon-tmp");
mkdirSync(tmp, { recursive: true });
const sizes = [16, 32, 48];
for (const s of sizes) await sharp(path.join(source, fallback.files.png2x)).resize(s, s).png().toFile(path.join(tmp, `${s}.png`));
execFileSync("magick", [...sizes.map((s) => path.join(tmp, `${s}.png`)), path.join(web, "public/favicon.ico")]);
rmSync(tmp, { recursive: true, force: true });

const catalog = {
  default: spec.default,
  categories: spec.categories.map((c) => ({ id: c.id, name: label(c.label) })),
  icons: spec.icons.map((i) => ({
    id: i.id,
    category: i.category,
    group: i.group ? label(i.group) : null,
    name: label(i.name),
  })),
};
mkdirSync(path.join(web, "src/app-icons"), { recursive: true });
writeFileSync(path.join(web, "src/app-icons/catalog.json"), JSON.stringify(catalog, null, 2) + "\n");
console.log(`${spec.icons.length} icons, default ${spec.default}`);
