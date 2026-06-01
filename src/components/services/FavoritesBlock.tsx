import { Star } from "lucide-react";

import { ServiceRow } from "@/components/services/ServiceRow";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { Device, Service } from "@/types";

export function FavoritesBlock({
  services,
  devices,
  isFavorite,
  onFavorite,
}: {
  services: Service[];
  devices: Device[];
  isFavorite: (service: Service) => boolean;
  onFavorite: (service: Service) => void;
}) {
  if (services.length === 0) return null;

  return (
    <Card className="overflow-hidden p-0">
      <CardHeader className="flex-row items-center gap-2 pb-2">
        <CardTitle className="text-warning">
          <Star className="size-4 fill-current" />
          Favorites
        </CardTitle>
        <span className="ml-auto text-xs text-muted-foreground">
          {services.length} pinned
        </span>
      </CardHeader>
      <CardContent className="p-1.5 pt-0">
        <div className="flex flex-col gap-0.5">
          {services.map((service) => (
            <ServiceRow
              key={`favorite-${service.id}`}
              service={service}
              devices={devices}
              favorite={isFavorite(service)}
              onFavorite={onFavorite}
            />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
