# vssh — brand tokens

Three concepts, one set of rules. Nothing is chosen yet: the owner picks one
concept, the other two are deleted, and this file loses its "per concept" rows.

The V stands for **Việt Nam**, so each concept carries one sign that says so
and that a version number never carries: a gold star, the marks of the "ệ" in
"Việt", or a flag on its pole.

## Files

| File | What | Use |
|---|---|---|
| `concept-N.svg` | the app icon: the glyph on its own rounded tile, 512 × 512 | the installed app (`.desktop`, hicolor), the README, anywhere the background is unknown |
| `concept-N-mark.svg` | the glyph alone, no tile, transparent, 512 × 512 | inside the app and on TokyoNight surfaces, where a tile on a panel would be a box in a box |
| `concept-N-full.svg` | the icon and the wordmark "vssh", 1340 × 512 | the README header, the site, an about dialog |
| `concept-N-preview-16.png`, `-512.png` | Chrome's render of `concept-N.svg` | proof only, not an asset |
| `preview-sheet.png` | all three, every variant, on `#16161e` and on white | proof only, not an asset |

| N | Name | The Vietnamese sign |
|---|---|---|
| 1 | the star in the V | a gold five-pointed star resting in the V's notch |
| 2 | the red seal | a seal's red tile; the V wears the hat and the dot below of "ệ" (the hat is also a nón lá) |
| 3 | the pole and the flag | the left arm is a pole, the right arm a red cloth in the wind with a gold star |

## Colour

All from TokyoNight Night, nothing new except the wordmark's blue.

| Token | Hex | Where | Contrast on `#16161e` | on `#1f2335` | on `#ffffff` |
|---|---|---|---|---|---|
| `brand-red` | `#f7768e` | the V (1), the tile (2), the cloth (3) | 6.80 | 5.88 | 2.65 |
| `brand-gold` | `#e0af68` | the star (1, 3) | 8.99 | 7.78 | 2.00 |
| `brand-blue` | `#7aa2f7` | the pole (3) | 7.14 | 6.18 | 2.52 |
| `brand-tile` | `#1f2335` | the tile (1, 3) | — | — | — |
| `brand-tile-edge` | `#292e42` | the tile's 8/512 inner edge (1, 3), so the tile does not vanish on `#16161e` | — | — | — |
| `brand-ink` | `#16161e` | the glyph on the red tile (2); on `#f7768e` it is 6.80 | — | — | — |
| `brand-wordmark` | `#6183e4` | "vssh" in `-full.svg` | 5.03 | 4.35 | 3.57 |

`brand-wordmark` is `#7aa2f7` darkened until it passes 3:1 on white as well as
on the dark surfaces, so one file serves both. Where the background is known,
a wordmark set in the page's own text colour is also fine: `#c0caf5` on dark,
`#1f2335` on light.

Red, gold and blue are below 3:1 on white. That is why the tile exists: on a
light or unknown background use `concept-N.svg`, never the bare mark of
concept 1 or 3. The bare mark of concept 2 is one colour and may be recoloured
to `#16161e` on light.

## Shape

| Token | Value |
|---|---|
| canvas | 512 × 512, `viewBox` only, no fixed `width`/`height` |
| tile radius | 112 of 512 (21.9%): 14px at 64px, 7px at 32px, in step with the app's 12–16px corners |
| glyph box | concept 1: 64–448 across, 87–428 down. Concept 2: 90–422 across, 50–468 down. Concept 3: 102–418 across, 86–424 down |
| strokes | round caps and round joins everywhere; the V is 68–72 of 512 thick |
| wordmark | "vssh", lower case, weight 700, size 300 of 512, tracking −6, baseline at 352; `Inter, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, 'Noto Sans', sans-serif`; live text, not outlines, no font embedded |
| gap, icon to wordmark | 80 of 512 |

## Safe area

- Inside the tile the glyph keeps at least 44 of 512 (about 9%, concept 2;
  64 and 86 for concepts 1 and 3) from every edge. Do not scale the glyph up inside the tile.
- Around the icon or the lockup, keep clear a quarter of the tile's side
  (128 of 512; 16px around a 64px icon). No text, no edge of a card inside it.
- The bare mark carries its own margin inside its 512 box: place the file,
  do not crop it.

## Minimum size

| Asset | Minimum | Tested |
|---|---|---|
| icon (`concept-N.svg`) | 16px | yes, `concept-N-preview-16.png` |
| bare mark | 16px on dark, 24px on light | 16px rendered on both in `preview-sheet.png`; on white it is pale, hence 24px |
| lockup (`-full.svg`) | 24px tall (63px wide) | yes, rendered at 63 × 24 |

Below 24px, concept 2's hat closes onto the V and concept 3's star is one or
two orange pixels; see the review in `RESULT-BRAND.md`.

## On TokyoNight

| Surface | Use |
|---|---|
| `#16161e` (window, the rail) | the icon, or the bare mark |
| `#1f2335` (panels, cards) | the bare mark for concepts 1 and 3, whose tile is this same colour; the icon for concept 2 |
| a light page, a store listing, a terminal's title bar | the icon |
| one colour only (a tray, a print) | the bare mark of concept 2, or the V of concept 1 without its star, in the surface's text colour |

## Do not

- redraw it as the flag: no red rectangle, no star centred on red;
- recolour the star to anything but `brand-gold`, or the V of concepts 1 and 3
  to blue or purple, which turns the V back into a version mark;
- add a shadow, a gradient or an outline; rotate or stretch it;
- set the wordmark in capitals ("VSSH") or in a serif or monospace face.
