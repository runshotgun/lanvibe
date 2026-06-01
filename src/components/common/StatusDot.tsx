import { cn } from "@/lib/utils";

export function StatusDot({
  active,
  loading = false,
  className,
}: {
  active: boolean;
  loading?: boolean;
  className?: string;
}) {
  const tone = loading ? "loading" : active ? "active" : "inactive";
  const colorClass =
    tone === "loading"
      ? "bg-[var(--status-loading)]"
      : tone === "active"
        ? "bg-[var(--status-active)]"
        : "bg-[var(--status-inactive)]";

  return (
    <span
      className={cn("relative flex size-2.5 shrink-0", className)}
      aria-hidden="true"
    >
      {tone !== "inactive" ? (
        <span
          className={cn(
            "absolute inline-flex size-full rounded-full opacity-60",
            tone === "loading" ? "animate-pulse" : "animate-ping",
            colorClass
          )}
        />
      ) : null}
      <span
        className={cn(
          "relative inline-flex size-2.5 rounded-full",
          colorClass
        )}
      />
    </span>
  );
}
