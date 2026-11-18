import type { ListItem } from "../api";
import type { Availability } from "../lib/available";
import { progressRatio } from "../lib/format";

/**
 * Watched episodes, with the episodes available in the library shown as a
 * lighter segment behind them.
 */
export function Progress({
  item,
  availability,
  flat = false,
}: {
  item: ListItem;
  availability?: Availability;
  flat?: boolean;
}) {
  const total = item.anime?.episodeCount ?? 0;
  const availableRatio = total > 0 && availability?.max ? Math.min(availability.max / total, 1) : 0;
  return (
    <span className={flat ? "progress flat" : "progress"}>
      {availableRatio > 0 && (
        <span className="progress-available" style={{ width: `${availableRatio * 100}%` }} />
      )}
      <span className="progress-watched" style={{ width: `${progressRatio(item) * 100}%` }} />
    </span>
  );
}
