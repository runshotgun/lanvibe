import { useState } from "react";
import { Globe } from "lucide-react";

export function ServiceFavicon({ url }: { url?: string | null }) {
  const [failed, setFailed] = useState(false);

  if (url && !failed) {
    return (
      <img
        src={url}
        alt=""
        className="size-8 rounded-md object-contain"
        onError={() => setFailed(true)}
      />
    );
  }

  return (
    <span className="grid size-8 place-items-center rounded-md bg-muted text-muted-foreground">
      <Globe className="size-4" />
    </span>
  );
}
