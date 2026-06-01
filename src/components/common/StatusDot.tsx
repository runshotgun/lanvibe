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

  return (
    <span
      className={cn(
        "status-dot relative flex size-4 shrink-0 items-center justify-center",
        className
      )}
      data-tone={tone}
      aria-hidden="true"
    >
      {tone === "active" ? (
        <span
          className="status-dot-pulse absolute inline-flex size-full animate-ping rounded-full opacity-55"
        />
      ) : null}
      <span className="status-dot-core relative inline-flex size-3 rounded-full" />
    </span>
  );
}
