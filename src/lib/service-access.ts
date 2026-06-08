import type { Service } from "@/types";

function normalizeHost(value?: string | null): string {
  return (value ?? "").trim().replace(/^\[|\]$/g, "").toLowerCase();
}

export function isLoopbackHost(value?: string | null): boolean {
  const host = normalizeHost(value);
  if (!host) return false;

  return (
    host === "localhost" ||
    host === "::1" ||
    host === "0:0:0:0:0:0:0:1" ||
    host === "127.0.0.1" ||
    host.startsWith("127.")
  );
}

export function isLoopbackService(service: Service): boolean {
  if (isLoopbackHost(service.ip)) return true;

  try {
    return isLoopbackHost(new URL(service.url).hostname);
  } catch {
    return false;
  }
}

export function canOpenLoopbackServicesFromHere(
  canOpenLoopbackServices?: boolean
): boolean {
  if (typeof canOpenLoopbackServices === "boolean") {
    return canOpenLoopbackServices;
  }

  if (typeof window === "undefined") return true;
  const host = window.location.hostname;

  // Tauri/custom-protocol contexts can have no ordinary LAN hostname, but they
  // run on the host machine and can open loopback URLs correctly.
  if (!host) return true;

  return isLoopbackHost(host);
}

export function isServiceUnavailableFromHere(
  service: Service,
  canOpenLoopbackServices?: boolean
): boolean {
  return (
    isLoopbackService(service) &&
    !canOpenLoopbackServicesFromHere(canOpenLoopbackServices)
  );
}

export function serviceOpenBlockReason(
  service: Service,
  canOpenLoopbackServices?: boolean
): string | null {
  if (!service.active) return "Service is inactive";

  if (isServiceUnavailableFromHere(service, canOpenLoopbackServices)) {
    return "Only available on the machine running LANVibe";
  }

  return null;
}

export function isServiceOpenable(
  service: Service,
  canOpenLoopbackServices?: boolean
): boolean {
  return serviceOpenBlockReason(service, canOpenLoopbackServices) === null;
}
