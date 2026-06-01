import type { ReactNode } from "react";
import { AppWindow, Network, SlidersHorizontal, Star } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { lanvibeLogoUrl } from "@/brand";
import { cn } from "@/lib/utils";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

export type Tab = "favorites" | "services" | "devices" | "settings";

interface NavItem {
  id: Tab;
  label: string;
  icon: LucideIcon;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "favorites",
    label: "Favorites",
    icon: Star,
  },
  {
    id: "services",
    label: "Services",
    icon: AppWindow,
  },
  {
    id: "devices",
    label: "Devices",
    icon: Network,
  },
  {
    id: "settings",
    label: "Settings",
    icon: SlidersHorizontal,
  },
];

export function AppShell({
  activeTab,
  onTabChange,
  error,
  children,
}: {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  error?: string | null;
  children: ReactNode;
}) {
  const activeIndex = Math.max(
    0,
    NAV_ITEMS.findIndex((item) => item.id === activeTab)
  );

  return (
    <div className="min-h-dvh md:flex">
      <div className="mobile-status-fade" aria-hidden="true" />
      <div className="mobile-nav-fade" aria-hidden="true" />

      {/* Desktop sidebar rail */}
      <aside className="safe-top sticky top-0 hidden h-dvh w-[4.5rem] shrink-0 flex-col items-center bg-card/40 px-3 pb-4 [--safe-top-offset:1rem] backdrop-blur-xl md:flex">
        <div className="flex pb-5">
          <span
            className="grid size-11 shrink-0 place-items-center overflow-hidden rounded-full bg-card shadow-soft ring-1 ring-border/70"
            aria-hidden="true"
          >
            <img
              src={lanvibeLogoUrl}
              alt=""
              className="size-full object-cover"
              draggable={false}
            />
          </span>
        </div>
        <nav className="flex flex-col items-center gap-2" aria-label="Main">
          {NAV_ITEMS.map((item) => {
            const active = item.id === activeTab;
            return (
              <Tooltip key={item.id}>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    onClick={() => onTabChange(item.id)}
                    aria-current={active ? "page" : undefined}
                    aria-label={item.label}
                    className={cn(
                      "grid size-11 place-items-center rounded-full text-sm font-medium transition-all duration-150 [&_svg]:size-[18px]",
                      active
                        ? "bg-primary/16 text-primary shadow-[inset_0_0_0_1px_hsl(var(--primary)/0.18)]"
                        : "text-muted-foreground hover:bg-card/70 hover:text-foreground hover:shadow-[inset_0_0_0_1px_hsl(var(--border)/0.55)]"
                    )}
                  >
                    <item.icon aria-hidden="true" />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right" align="center">
                  {item.label}
                </TooltipContent>
              </Tooltip>
            );
          })}
        </nav>
      </aside>

      {/* Main column */}
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="safe-top safe-x mx-auto w-full max-w-5xl flex-1 pb-[calc(env(safe-area-inset-bottom,0px)+8rem)] [--safe-left-offset:1rem] [--safe-right-offset:1rem] [--safe-top-offset:1rem] sm:[--safe-left-offset:1.5rem] sm:[--safe-right-offset:1.5rem] md:pb-10">
          {error ? (
            <div className="mb-4 rounded-xl border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm font-medium text-destructive">
              {error}
            </div>
          ) : null}
          {children}
        </main>
      </div>

      {/* Mobile bottom tab bar */}
      <nav
        className="safe-bottom safe-x pointer-events-none fixed inset-x-0 bottom-0 z-40 [--safe-bottom-offset:0.75rem] [--safe-left-offset:1rem] [--safe-right-offset:1rem] md:hidden"
        aria-label="Main"
      >
        <div className="tactile pointer-events-auto relative isolate mx-auto grid min-h-16 max-w-[25rem] grid-cols-4 overflow-hidden rounded-full p-1.5">
          <span
            className="btn-primary absolute bottom-1.5 left-1.5 top-1.5 z-0 w-[calc((100%-0.75rem)/4)] rounded-full transition-transform duration-300 ease-out"
            style={{ transform: `translateX(${activeIndex * 100}%)` }}
            aria-hidden="true"
          />
          {NAV_ITEMS.map((item) => {
            const active = item.id === activeTab;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => onTabChange(item.id)}
                aria-current={active ? "page" : undefined}
                aria-label={item.label}
                className={cn(
                  "relative z-10 flex min-w-0 items-center justify-center rounded-full transition-colors [&_svg]:size-5",
                  active
                    ? "text-primary-foreground"
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                <item.icon aria-hidden="true" />
              </button>
            );
          })}
        </div>
      </nav>
    </div>
  );
}
