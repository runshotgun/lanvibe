import * as React from "react";

import { cn } from "@/lib/utils";

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, disabled, ...props }, ref) => {
    return (
      <span
        className={cn(
          "tactile inline-flex h-10 w-full items-center rounded-full px-4 text-sm font-bold transition-all focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-1 focus-within:ring-offset-background",
          disabled && "cursor-not-allowed opacity-55",
          className
        )}
      >
        <input
          type={type}
          ref={ref}
          disabled={disabled}
          className="h-full min-w-0 flex-1 border-0 bg-transparent p-0 text-sm font-bold text-inherit outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed"
          {...props}
        />
      </span>
    );
  }
);
Input.displayName = "Input";

export { Input };
