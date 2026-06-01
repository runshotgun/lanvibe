import { useId } from "react";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function NumberField({
  label,
  value,
  min,
  max,
  hint,
  onChange,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  hint?: string;
  onChange: (value: number) => void;
}) {
  const id = useId();
  return (
    <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
      <div className="min-w-0">
        <Label
          htmlFor={id}
          className="truncate text-base font-semibold leading-tight"
        >
          {label}
        </Label>
        {hint ? (
          <p className="mt-1 truncate text-sm leading-tight text-muted-foreground">
            {hint}
          </p>
        ) : null}
      </div>
      <Input
        id={id}
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="h-10 w-24 shrink-0 px-3 text-right text-base sm:w-28"
      />
    </div>
  );
}
