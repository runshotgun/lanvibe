import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

export function EmptyState({
  icon,
  title,
  body,
  className,
}: {
  icon: ReactNode;
  title: string;
  body?: string;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-52 flex-col items-center justify-center gap-3 px-6 py-12 text-center",
        className
      )}
    >
      <div className="grid size-14 place-items-center rounded-2xl bg-muted/60 text-muted-foreground [&_svg]:size-7">
        {icon}
      </div>
      <p className="text-base font-semibold text-foreground">{title}</p>
      {body ? (
        <p className="max-w-xs text-sm text-muted-foreground">{body}</p>
      ) : null}
    </div>
  );
}
