# LANVibe Agent Guide

Tauri 2 + React 18 + Vite + TypeScript. The single Vite build (`dist/`) is served both in the desktop window and over the LAN by the Rust backend, so the frontend must stay responsive and bundle-conscious.

## UI / Branding Required Reading Before Any Frontend Work

All web UI work (components, pages, layouts, colors, spacing, theming, responsive/mobile/menu-bar behavior) MUST follow the design system in:

- [.cursor/skills/ui-branding/SKILL.md](.cursor/skills/ui-branding/SKILL.md)

Key non-negotiables (see the skill for full detail):

- Tailwind v4 + shadcn/ui ("new-york"); style via semantic tokens, never hardcoded hex/rgb.
- "Liquid glass" depth via the `.glass` / `.glass-strong` utilities and the `<Card>` primitive.
- Must look correct in light, dark, and system themes, and work from ~390px mobile up to desktop.
- Reuse the shell in `src/components/layout/AppShell.tsx`; keep views usable in a narrow single column (menu-bar popover).

## Commands

```bash
npm install
npm run dev      # Vite dev server on :1420
npm run build    # tsc + vite build (must pass clean)
npm run tauri -- dev
```

## Conventions

- Path alias `@/` -> `src/`. Feature folders under `src/components/{services,devices,settings,layout,common}`; shared logic in `src/hooks/` and `src/lib/`.
- Components stay presentational; data fetching/polling/mutations go through hooks (e.g. `useFinderData`). Backend access stays in `src/api.ts`.
