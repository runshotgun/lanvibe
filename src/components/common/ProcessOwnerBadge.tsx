import { Cpu } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function ProcessOwnerBadge({
  owner,
  className,
}: {
  owner?: string | null;
  className?: string;
}) {
  const label = owner?.trim();
  if (!label) return null;

  return (
    <Badge
      variant="muted"
      title={`Port owned by ${label}`}
      className={cn(
        "min-w-0 max-w-full px-1.5 py-0 text-[10px] font-medium",
        className
      )}
    >
      <Cpu className="size-3 shrink-0" />
      <span className="truncate">{label}</span>
    </Badge>
  );
}
