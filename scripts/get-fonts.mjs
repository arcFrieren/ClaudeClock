// Descarga las fuentes woff2 (subset latin) de Google Fonts al directorio src/fonts.
// Solo se ejecuta en desarrollo; en runtime la app no toca la red para fuentes (SPEC §2).
import { writeFile } from "node:fs/promises";
import path from "node:path";

const UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

const FAMILIES = [
  ["Geist Mono", [300, 500]],
  ["JetBrains Mono", [400, 700]],
  ["DM Mono", [400, 500]],
  ["Barlow Condensed", [400, 600, 700]],
  ["Atkinson Hyperlegible Mono", [400, 700]],
  ["SUSE Mono", [400, 700]],
];

const outDir = path.join(import.meta.dirname, "..", "src", "fonts");
const slug = (f) => f.toLowerCase().replace(/ /g, "-");

for (const [family, weights] of FAMILIES) {
  const famParam = family.replace(/ /g, "+") + ":wght@" + weights.join(";");
  const url = `https://fonts.googleapis.com/css2?family=${famParam}&display=swap`;
  const css = await (await fetch(url, { headers: { "User-Agent": UA } })).text();

  // Bloques @font-face: nos quedamos con el subset /* latin */ de cada peso.
  const blocks = css.split("/*").filter((b) => b.startsWith(" latin */"));
  for (const block of blocks) {
    const weight = block.match(/font-weight:\s*(\d+)/)?.[1];
    const src = block.match(/url\((https:[^)]+\.woff2)\)/)?.[1];
    if (!weight || !src) continue;
    const file = `${slug(family)}-${weight}.woff2`;
    const buf = Buffer.from(await (await fetch(src)).arrayBuffer());
    await writeFile(path.join(outDir, file), buf);
    console.log(`ok ${file} (${buf.length} bytes)`);
  }
}
console.log("fonts done");
