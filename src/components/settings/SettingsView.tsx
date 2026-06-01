import { Activity, Check, Loader2 } from "lucide-react";

import { DashboardCard } from "@/components/settings/DashboardCard";
import { NumberField } from "@/components/settings/NumberField";
import { UpdateCard } from "@/components/settings/UpdateCard";
import { ThemeToggle } from "@/components/theme-toggle";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import type { Settings, SettingsView as SettingsViewModel } from "@/types";

export function SettingsView({
  value,
  saving,
  onChange,
}: {
  value: SettingsViewModel;
  saving: boolean;
  onChange: (settings: Settings) => void;
}) {
  const settings = value.settings;

  function update<K extends keyof Settings>(key: K, next: Settings[K]) {
    onChange({ ...settings, [key]: next });
  }

  const autoScanEnabled = settings.autoScan && !settings.manualOnly;

  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <Card>
        <CardHeader className="p-3 pb-2 sm:p-4 sm:pb-2">
          <CardTitle>
            <Activity className="size-4 text-primary" />
            Scanning
            <span className="ml-auto inline-flex items-center gap-1.5 text-sm font-normal text-muted-foreground">
              {saving ? (
                <>
                  <Loader2 className="size-3.5 animate-spin" />
                  Saving
                </>
              ) : (
                <>
                  <Check className="size-3.5 text-success" />
                  Saved
                </>
              )}
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col divide-y divide-border/60 p-0">
          <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
            <div className="min-w-0">
              <Label className="text-base font-semibold leading-tight">
                Automatic scans
              </Label>
              <p className="mt-1 truncate text-sm leading-tight text-muted-foreground">
                Periodically re-scan selected devices
              </p>
            </div>
            <Switch
              checked={autoScanEnabled}
              onCheckedChange={(checked) =>
                onChange({
                  ...settings,
                  autoScan: checked,
                  manualOnly: !checked,
                })
              }
              aria-label="Automatic scans"
            />
          </div>
          <NumberField
            label="Scan interval"
            hint="seconds"
            min={30}
            value={settings.scanIntervalSeconds}
            onChange={(next) => update("scanIntervalSeconds", next)}
          />
          <NumberField
            label="Discovery interval"
            hint="seconds"
            min={30}
            value={settings.discoveryIntervalSeconds}
            onChange={(next) => update("discoveryIntervalSeconds", next)}
          />
          <NumberField
            label="Retention"
            hint="days inactive services are kept"
            min={1}
            value={settings.retentionDays}
            onChange={(next) => update("retentionDays", next)}
          />
          <NumberField
            label="Concurrent probes"
            min={32}
            value={settings.scanConcurrency}
            onChange={(next) => update("scanConcurrency", next)}
          />
          <NumberField
            label="Connect timeout"
            hint="ms"
            min={100}
            value={settings.connectTimeoutMs}
            onChange={(next) => update("connectTimeoutMs", next)}
          />
          <NumberField
            label="HTTP timeout"
            hint="ms"
            min={250}
            value={settings.httpTimeoutMs}
            onChange={(next) => update("httpTimeoutMs", next)}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="p-3 pb-2 sm:p-4 sm:pb-2">
          <CardTitle>
            <Activity className="size-4 text-primary" />
            Desktop
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col divide-y divide-border/60 p-0">
          <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
            <div className="min-w-0">
              <Label className="text-base font-semibold leading-tight">
                Launch at startup
              </Label>
              <p className="mt-1 truncate text-sm leading-tight text-muted-foreground">
                Start LANVibe in the tray when you sign in
              </p>
            </div>
            <Switch
              checked={settings.launchAtStartup}
              onCheckedChange={(checked) => update("launchAtStartup", checked)}
              aria-label="Launch at startup"
            />
          </div>
          <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
            <div className="min-w-0">
              <Label className="text-base font-semibold leading-tight">
                Close to tray
              </Label>
              <p className="mt-1 truncate text-sm leading-tight text-muted-foreground">
                Keep LANVibe running when the window is closed
              </p>
            </div>
            <Switch
              checked={settings.minimizeToTray}
              onCheckedChange={(checked) => update("minimizeToTray", checked)}
              aria-label="Close to tray"
            />
          </div>
          <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
            <div className="min-w-0">
              <Label className="text-base font-semibold leading-tight">
                Appearance
              </Label>
              <p className="mt-1 truncate text-sm leading-tight text-muted-foreground">
                Choose the dashboard color mode
              </p>
            </div>
            <ThemeToggle />
          </div>
          <p className="px-3 py-3 text-sm leading-snug text-muted-foreground sm:px-4">
            Use the tray menu Quit item when you want to fully exit.
          </p>
        </CardContent>
      </Card>

      <UpdateCard />

      <DashboardCard value={value} onChange={update} />
    </div>
  );
}
