import SortableList, { SortableItem } from "react-easy-sort";

import { FavoriteTile, serviceOrigin } from "@/components/favorites/FavoriteTile";
import { serviceKey } from "@/lib/finder";
import type { Device, Service } from "@/types";

function arrayMove<T>(list: T[], from: number, to: number): T[] {
  const next = [...list];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

export function ReorderableFavoritesGrid({
  services,
  devices,
  favicons,
  compact = false,
  onReorder,
  onOpen,
}: {
  services: Service[];
  devices: Device[];
  favicons: Record<string, string | null | undefined>;
  compact?: boolean;
  onReorder: (orderedKeys: string[]) => void;
  onOpen: (service: Service) => void;
}) {
  const handleSortEnd = (oldIndex: number, newIndex: number) => {
    if (oldIndex === newIndex) return;
    const reordered = arrayMove(services, oldIndex, newIndex);
    onReorder(reordered.map(serviceKey));
  };

  return (
    <SortableList
      onSortEnd={handleSortEnd}
      className="favorite-reorder-grid grid grid-cols-2 gap-2 sm:gap-3"
      draggedItemClassName="favorite-tile-dragging"
    >
      {services.map((service) => (
        <SortableItem key={service.id}>
          <div className="h-full">
            <FavoriteTile
              service={service}
              devices={devices}
              favicon={favicons[serviceOrigin(service)]}
              compact={compact}
              editing
              onOpen={onOpen}
            />
          </div>
        </SortableItem>
      ))}
    </SortableList>
  );
}
