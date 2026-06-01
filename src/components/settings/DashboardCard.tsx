import { ExternalLink, Globe2, ShieldOff } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";

import { inTauri, openService } from "@/api";
import { NumberField } from "@/components/settings/NumberField";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTheme } from "@/components/theme-provider";
import type { Settings, SettingsView } from "@/types";

export function DashboardCard({
  value,
  onChange,
}: {
  value: SettingsView;
  onChange: <K extends keyof Settings>(key: K, next: Settings[K]) => void;
}) {
  const { resolvedTheme } = useTheme();
  const settings = value.settings;
  const dashboardUrl =
    value.dashboardUrls[0] ?? `http://localhost:${value.actualDashboardPort}`;

  return (
    <Card>
      <CardHeader className="p-3 pb-2 sm:p-4 sm:pb-2">
        <CardTitle>
          <Globe2 className="size-4 text-primary" />
          LAN dashboard
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col divide-y divide-border/60 p-0">
        <div className="flex min-h-14 items-center justify-between gap-3 px-3 py-3 sm:min-h-16 sm:px-4">
          <Label
            htmlFor="dashboard-bind"
            className="shrink-0 text-base font-semibold leading-tight"
          >
            Bind address
          </Label>
          <Input
            id="dashboard-bind"
            value={settings.dashboardBind}
            onChange={(event) => onChange("dashboardBind", event.target.value)}
            className="h-10 min-w-0 flex-1 px-3 text-base"
          />
        </div>
        <NumberField
          label="Preferred port"
          min={1}
          max={65535}
          value={settings.dashboardPort}
          onChange={(next) => onChange("dashboardPort", next)}
        />

        <div className="flex items-center gap-3 px-3 py-3 sm:px-4">
          <div className="shrink-0 rounded-lg bg-[var(--color-ash-grey-50)] p-1.5 shadow-soft">
            <QRCodeSVG
              value={dashboardUrl}
              size={76}
              bgColor="#f0f4f2"
              fgColor={resolvedTheme === "dark" ? "#141204" : "#141204"}
            />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-base font-semibold leading-tight">
              {dashboardUrl}
            </p>
            <p className="mt-1 flex items-center gap-1.5 text-sm leading-tight text-muted-foreground">
              <ShieldOff className="size-4" />
              Open on your LAN
            </p>
          </div>
        </div>

        {value.dashboardUrls.length > 0 ? (
          <div className="flex flex-col gap-2 px-3 py-3 sm:px-4">
            {value.dashboardUrls.map((url) => (
              <Button
                key={url}
                variant="tactile"
                asChild
                className="h-10 justify-between rounded-full px-3 text-primary"
              >
                <a
                  href={url}
                  target="_blank"
                  rel="noopener noreferrer external"
                  onClick={(event) => {
                    if (!inTauri()) return;
                    event.preventDefault();
                    void openService(url);
                  }}
                >
                  <span className="truncate">{url}</span>
                  <ExternalLink className="size-3.5 shrink-0" />
                </a>
              </Button>
            ))}
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
