# PinIt design system

Direction: **quiet instrument.** The images are the content; the app chrome should feel like a
precise tool — calm neutrals, one accent, generous spacing, no gradients, no emoji, no decoration
that does not carry meaning. Native-feeling density (Windows 11 / Linear / Raycast territory),
not a marketing page.

## Principles
1. One accent color, used only for selection, focus and primary action.
2. Hierarchy by weight, spacing and contrast — not by boxes inside boxes.
3. Every interactive thing has hover, active, focus-visible and disabled states.
4. Icons are one family: 24px grid, 1.75 stroke, round caps/joins, no fills.
5. Motion is short (120-180 ms), ease-out, and never blocks input.
6. Light and dark are first-class; "system" follows the OS. Viewer overlays are always dark glass
   because they sit on arbitrary images.

## Tokens (see `src/design.css`)
| token | light | dark |
|---|---|---|
| `--bg` | #f5f5f3 | #0d0e11 |
| `--surface` | #ffffff | #15171b |
| `--surface-2` | #f0f0ed | #1c1f24 |
| `--text` | #15171c | #eceef1 |
| `--text-muted` | #6a707b | #8b919c |
| `--border` | rgba(15,18,25,.10) | rgba(255,255,255,.08) |
| `--accent` | #3d5afe | #8195ff |
| `--accent-fg` | #ffffff | #0d0e11 |
| `--danger` | #d6403a | #ff7b72 |

Radii 8 / 12 / 999. Spacing scale 4 8 12 16 24 32. Type: Segoe UI Variable / system-ui, 13px
body, 12px captions, 20px title (600). No web fonts (zero network, zero dependency).

## Components
Segmented control, switch, chip, slider, card (surface + 1px border, no shadow), anchor picker
(mini screen with corner targets), toast. Buttons: primary (accent fill), secondary (surface-2),
ghost (text only), icon (28px square, 8px radius).

## Viewer overlay
Dark glass (`rgba(20,22,26,.72)` + blur), appears on hover, hides on leave. Top-right close,
bottom-center tool pill: rotate ←/→, flip H/V, zoom −/+, reset, save, open source. An angle chip
appears when free rotation is non-zero (click to reset).

## Keyboard (viewer)
hold `R` + drag rotate about the window centre · `H` / `V` flip about the window centre (screen axes) · `Alt+wheel` free rotate (Shift = 15° steps) ·
wheel zoom · `Space+drag` or middle-drag pan · `0` reset · `Esc` close overlay focus.
