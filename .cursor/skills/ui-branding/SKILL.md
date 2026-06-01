---
name: ui-branding
description: Brand and design-system guide for the LAN Web UI Finder frontend (Tailwind v4 + shadcn/ui, "liquid glass" depth, light/dark/system themes). Use when building, restyling, or reviewing any web UI in this repo - components, pages, layouts, colors, spacing, theming, or responsive/mobile/menu-bar behavior.
---

# LAN Web UI Finder - UI Branding & Design System

Authoritative guide for the frontend look and feel. Follow it for every UI change so the desktop window, the LAN dashboard, and the future menu-bar popover stay visually consistent. Keep changes subtle, minimalist, physical.

## Design philosophy

- **Subtle liquid glass.** Translucent surfaces, hairline borders, soft layered shadows, generous radius. Depth and physicality without noise.
- **Minimalist & calm.** One azure accent (deep-space-blue) over a cool sky-blue neutral palette, lots of breathing room. Decoration must earn its place.
- **Readable & dense where it counts.** Maximize usable space on both desktop and mobile; never sacrifice legibility for style.
- **Same UI everywhere.** It ships to the Tauri window, LAN phones/tablets, and a menu-bar popover. Design responsive and compact-friendly by default.

## Stack (do not reinvent)

- Tailwind CSS **v4** (`@tailwindcss/vite`, configured in [vite.config.ts](vite.config.ts)). No `tailwind.config.js`; theme lives in CSS.
- **shadcn/ui**, "new-york" style ([components.json](components.json)). Primitives in `src/components/ui/`.
- Icons: **lucide-react** only. QR: `qrcode.react`.
- `cn()` from [src/lib/utils.ts](src/lib/utils.ts) for class merging. Path alias `@/` -> `src/`.

## Tokens — never hardcode colors

All color/radius/shadow tokens are CSS variables defined in [src/styles.css](src/styles.css) (oklch, with a `.dark` override block). Always style via the **semantic Tailwind classes**, never raw hex/rgb.

| Purpose | Class examples |
|---|---|
| Page / base text | `bg-background`, `text-foreground` |
| Cards / popovers | `bg-card`, `text-card-foreground`, `bg-popover` |
| Accent (azure) | `bg-primary`, `text-primary`, `text-primary-foreground` |
| Subdued surfaces | `bg-secondary`, `bg-muted`, `text-muted-foreground` |
| Hover/active tint | `bg-accent`, `text-accent-foreground` |
| Status | `text-success` / `bg-success`, `text-warning`, `text-destructive` |
| Lines / inputs | `border-border`, `border-input`, focus ring `ring-ring` |

Radius scale keys off `--radius` (0.85rem): use `rounded-lg` (default control), `rounded-xl` (cards), `rounded-full` (pills/dots).

Palette families (raw ramps in [src/styles.css](src/styles.css), wired into the semantic tokens above):

- **sky-blue** — cool neutral base: backgrounds, foreground text, cards, muted, borders, glass.
- **deep-space-blue** — `primary`/brand accent + focus `ring`.
- **blue-green** — `success`.
- **amber-flame** — `warning`.
- **princeton-orange** — `destructive`.

Rules:
- Need a new color? Add a token to **both** `:root` and `.dark` in [src/styles.css](src/styles.css), expose it in the `@theme inline` block, then use the class. Do not introduce one-off hex values in components.
- Status uses semantic tokens (`success`/`warning`/`destructive`), not literal green/yellow/red.

## Glass & elevation

Use the prebuilt component classes from [src/styles.css](src/styles.css) for depth; do not hand-roll `backdrop-filter`.

- `.glass` — standard translucent surface + hairline border + soft shadow. Cards, chips, secondary buttons.
- `.glass-strong` — more opaque + stronger shadow. Floating layers: dropdowns, tooltips, popovers.
- `.shadow-soft` / `.shadow-raised` — elevation only, no background.

The `<Card>` primitive already applies `.glass`. Headers/footers/nav bars use a translucent token bg + `backdrop-blur-xl` (e.g. `bg-background/70 backdrop-blur-xl`).

## Theming (light / dark / system)

- Driven by [src/components/theme-provider.tsx](src/components/theme-provider.tsx); it toggles the `.dark` class on `<html>`, persists to `localStorage`, follows `matchMedia` for "system", and updates `<meta theme-color>`.
- Read state with `useTheme()`; switch UI is [src/components/theme-toggle.tsx](src/components/theme-toggle.tsx).
- Every new surface MUST look correct in all three modes. Because you use semantic tokens, this is automatic — verify it, don't special-case dark with conditional classes unless truly necessary (e.g. QR foreground color).

## Typography & spacing

- Font stack: Inter / system UI (set on `body`). Don't import other fonts.
- Sizes: page/section title `text-lg font-semibold tracking-tight`; card title `text-sm font-semibold`; body `text-sm`; meta/caption `text-xs text-muted-foreground`. Use `tabular-nums` for counts/ports.
- Spacing rhythm: list/section gaps `gap-3`/`gap-4`; card padding `p-4 sm:p-5`; control height `h-10` (`h-9` compact). Touch targets >= ~36-44px.

## Layout & responsiveness

- The shell is [src/components/layout/AppShell.tsx](src/components/layout/AppShell.tsx): glass **sidebar rail + sticky header** at `md+`, collapsing to a **glass bottom tab bar** below `md`. Content is centered at `max-w-5xl`.
- Build mobile-first; verify at ~390px and at desktop. New top-level views render inside the shell — reuse it, don't create competing chrome.
- **Safe areas:** wrap edge-touching containers with the `.safe-top` / `.safe-bottom` / `.safe-x` helpers so the installed webapp respects notches/home indicators.
- **Popover-ready:** keep views usable in a narrow single column (the menu-bar window). Avoid fixed widths and wide multi-column layouts that can't collapse.

## Components

- Reach for shadcn primitives in `src/components/ui/` first: `Button`, `Input`, `Card`, `Switch`, `Badge`, `Separator`, `Label`, `Tooltip`, `ScrollArea`, `DropdownMenu`, `Skeleton`.
- `Button` variants: `default` (teal CTA), `outline`, `secondary`, `ghost`, `glass`, `destructive`, `link`; sizes incl. `icon` / `icon-sm`. One primary action per view.
- `Badge` variants: `default`, `secondary`, `outline`, `success`, `warning`, `muted`.
- Loading uses `<Skeleton>` or a spinning lucide `Loader2` (`animate-spin`); empty states use [src/components/common/EmptyState.tsx](src/components/common/EmptyState.tsx); live status uses [src/components/common/StatusDot.tsx](src/components/common/StatusDot.tsx).
- Need a primitive that's missing? Add it via `npx shadcn@latest add <name>` (new-york). Match the existing files' token-based styling.

## Iconography

- lucide-react, default `size-4` (16px) inline, `size-5` for emphasis; let `currentColor` inherit. Keep icon usage meaningful and sparse.

## Architecture conventions

- Feature folders: `src/components/{services,devices,settings,layout,common}/`. Shared logic in `src/hooks/` and `src/lib/`. Keep `App.tsx` a thin composition.
- Data/format helpers live in [src/lib/finder.ts](src/lib/finder.ts); don't duplicate formatting. Backend access stays in [src/api.ts](src/api.ts).
- Components stay presentational; fetch/poll/mutate via hooks like `useFinderData`.

## Do / Don't

- DO use semantic token classes, glass utilities, shadcn primitives, and the shell.
- DO verify light + dark + system and mobile + desktop before finishing.
- DON'T hardcode hex/rgb, add new fonts, hand-roll glass/blur, build bespoke buttons/inputs, or add heavy UI deps without asking.
- DON'T break the compact/popover single-column constraint.

## Pre-merge checklist

- [ ] Colors/spacing/radius use semantic tokens (no literals)
- [ ] Surfaces use `.glass`/`Card`; floating layers use `.glass-strong`
- [ ] Correct in light, dark, and system
- [ ] Works at ~390px and desktop; safe-area helpers on edge containers
- [ ] Uses existing shadcn primitives + shell; one primary action per view
- [ ] Loading + empty states handled
