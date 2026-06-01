import { FormEvent, useEffect, useRef, useState } from "react";
import { Check, Pencil, X } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { deviceName } from "@/lib/finder";
import { cn } from "@/lib/utils";
import type { Device } from "@/types";

export function DeviceRow({
  device,
  onToggle,
  onRename,
}: {
  device: Device;
  onToggle: (device: Device, selected: boolean) => void;
  onRename: (device: Device, name: string | null) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!editing) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [editing]);

  function beginRename() {
    setDraft(device.nameOverride || device.hostname || device.ip);
    setEditing(true);
  }

  function cancelRename() {
    setEditing(false);
    setDraft("");
  }

  function saveRename(event?: FormEvent) {
    event?.preventDefault();
    onRename(device, draft.trim() || null);
    setEditing(false);
  }

  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-xl px-4 py-3 transition-colors",
        device.selected ? "bg-primary/[0.07]" : "hover:bg-accent/40"
      )}
    >
      <div className="min-w-0 flex-1">
        {editing ? (
          <form className="flex items-center gap-2" onSubmit={saveRename}>
            <Input
              ref={inputRef}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              className="h-9 min-w-0"
              aria-label={`Display name for ${device.ip}`}
            />
            <Button type="submit" size="icon-sm" aria-label="Save name">
              <Check />
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Cancel rename"
              onClick={cancelRename}
            >
              <X />
            </Button>
          </form>
        ) : (
          <button
            type="button"
            className="block max-w-full truncate text-left text-sm font-semibold underline-offset-4 hover:underline"
            onClick={beginRename}
            title="Rename device"
          >
            {deviceName(device)}
          </button>
        )}
        <p className="truncate text-xs text-muted-foreground">
          {device.ip}
          {device.mac ? ` / ${device.mac}` : ""}
          {device.vendor ? ` / ${device.vendor}` : ""}
        </p>
      </div>
      {!editing ? (
        <button
          type="button"
          onClick={beginRename}
          aria-label={`Rename ${deviceName(device)}`}
          title="Rename device"
          className="grid size-9 shrink-0 place-items-center rounded-lg border border-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        >
          <Pencil className="size-4" />
        </button>
      ) : null}
      <Badge variant="outline" className="hidden capitalize sm:inline-flex">
        {device.source}
      </Badge>
      <Switch
        checked={device.selected}
        onCheckedChange={(checked) => onToggle(device, checked)}
        aria-label={`Scan ${deviceName(device)}`}
      />
    </div>
  );
}
