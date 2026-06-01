import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

import App from "./App";
import { PopoverApp } from "@/components/popover/PopoverApp";
import { ThemeProvider } from "@/components/theme-provider";
import { TooltipProvider } from "@/components/ui/tooltip";
import "./styles.css";

function currentWindowLabel(): string {
  if (new URLSearchParams(window.location.search).get("window") === "popover") {
    return "popover";
  }
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) return "main";
  try {
    return getCurrentWindow().label;
  } catch {
    return "main";
  }
}

const isPopover = currentWindowLabel() === "popover";
if (isPopover) {
  document.documentElement.dataset.window = "popover";
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <TooltipProvider delayDuration={200}>
        {isPopover ? <PopoverApp /> : <App />}
      </TooltipProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
