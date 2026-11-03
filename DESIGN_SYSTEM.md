# Design system — desktop UI

Tokens live in `apps/desktop/ui/styles.css`. Change them there, not in components.

## Colour

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#ffffff` | `#212121` | main surface |
| `--bg-raised` | `#f9f9f9` | `#171717` | sidebar, inspector, message cards |
| `--bg-sunken` | `#f3f3f3` | `#2a2a2a` | code blocks, pills |
| `--fg` | `#0d0d0d` | `#ececec` | text |
| `--fg-muted` | `#5d5d5d` | `#a8a8a8` | secondary text, labels |
| `--line` | `#e5e5e5` | `#3a3a3a` | hairlines |
| `--accent` | `#10a37f` | `#19c37d` | primary action, focus ring — **nothing else** |
| `--danger` | `#b3261e` | `#ff8a80` | errors |

One accent colour. If a second colour appears, one of the two is decoration.

## Type and spacing

System font stack, 15px base, 1.55 line-height. Mono for file contents and hashes.
Radii: 12px cards, 16px messages and composer, 999px pills and buttons.
Layout: sidebar 260px · content column capped at 52rem · inspector 400px.

## Layout rules

- Three columns: project rail, conversation column, inspector.
- The composer is pinned to the bottom; the thread scrolls above it.
- Surfaces are separated by **tone**, with a hairline only where two surfaces of the same
  tone meet.
- Breakpoints: below 1200px the inspector is hidden; below 820px the sidebar is hidden.

## Accessibility — non-negotiable

- Every input has a visible label (`sr-only` where the design wants none).
- `:focus-visible` ring on every interactive element.
- `role="status" aria-live="polite"` for progress; `role="alert"` for errors.
- Skip link to `#main` as the first focusable element.
- Status is never carried by colour alone: pills carry text.

## Verified by

`node scripts/verify-ui.mjs` — 38 checks including labels, focus styles, live regions and the
absence of an error banner. **A passing DOM check does not prove the layout** — render it and
look (`--screenshot`), which is how the stacked device-facts and the overflowing model table
were caught.
