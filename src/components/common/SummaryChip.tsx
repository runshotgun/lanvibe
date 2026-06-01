import type { LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

export function SummaryChip({
  icon: Icon,
  value,
  label,
  tone = "default",
}: {
  icon: LucideIcon;
  value: number | string;
  label: string;
  tone?: "default" | "success";
}) {
  return (
    <div className="glass flex items-center gap-2 rounded-full py-1.5 pl-2.5 pr-3.5">
      <span
        className={cn(
          "grid size-6 place-items-center rounded-full [&_svg]:size-3.5",
          tone === "success"
            ? "bg-success/15 text-success"
            : "bg-primary/12 text-primary"
        )}
      >
        <Icon />
      </span>
      <span className="text-sm font-semibold tabular-nums text-foreground">
        {value}
      </span>
      <span className="hidden text-xs text-muted-foreground sm:inline">
        {label}
      </span>
    </div>
  );
}
