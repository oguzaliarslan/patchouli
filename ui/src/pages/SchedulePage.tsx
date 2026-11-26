import { useMemo, useState } from "react";
import type { ListItem } from "../api";
import { Poster } from "../components/Poster";
import { countdown, followsAiring, nextAiring, useNow, type NextAiring } from "../lib/airing";
import { useApp } from "../lib/context";
import { isNsfw, isWatching, itemTitle } from "../lib/format";
import { usePref } from "../lib/prefs";
import { useList, useSettings } from "../lib/queries";
import { useAnimeMenu } from "../lib/useAnimeMenu";

type Scope = "watching" | "list";
type View = "week" | "month" | "list";

const WEEK_SECONDS = 7 * 86_400;

interface Row {
  item: ListItem;
  next: NextAiring;
}

/** One episode on the calendar. */
interface Slot {
  item: ListItem;
  episode: number;
  /** Unix seconds. */
  time: number;
  /** Projected weekly from the reported next airing, not reported by the service. */
  estimate: boolean;
  /** Aired episodes before this one that haven't been watched (only on the reported next airing). */
  behind: number;
}

function dayLabel(seconds: number, now: number): string {
  const date = new Date(seconds * 1000);
  const today = new Date(now * 1000);
  const tomorrow = new Date(today);
  tomorrow.setDate(today.getDate() + 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === tomorrow.toDateString()) return "Tomorrow";
  return date.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
}

/** The locale's first day of the week, as `Date.getDay()` numbers it (0 is Sunday). Monday if unknown. */
function firstDayOfWeek(): number {
  try {
    const locale = new Intl.Locale(navigator.language) as Intl.Locale & {
      weekInfo?: { firstDay: number };
      getWeekInfo?: () => { firstDay: number };
    };
    const info = locale.getWeekInfo?.() ?? locale.weekInfo;
    if (info) return info.firstDay % 7;
  } catch {
    // Older engines: fall through.
  }
  return 1;
}

/** Local midnight at the start of `date`'s day. */
function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function addDays(date: Date, days: number): Date {
  // Calendar days, so a DST change doesn't move midnight.
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

function startOfWeek(date: Date, firstDay: number): Date {
  const day = startOfDay(date);
  return addDays(day, -((day.getDay() - firstDay + 7) % 7));
}

const seconds = (date: Date) => Math.floor(date.getTime() / 1000);
const dayKey = (date: Date) => date.toDateString();
const timeText = (time: number) =>
  new Date(time * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/**
 * Episodes airing in [start, end). Only the next airing is known, so the
 * others are projected a week apart from it (in real seconds: airings are
 * fixed in Japan time, which has no DST), within episodes 1 to the count.
 */
function slotsBetween(rows: Row[], start: number, end: number): Slot[] {
  const slots: Slot[] = [];
  for (const { item, next } of rows) {
    const total = item.anime?.episodeCount ?? 0;
    const first = Math.ceil((start - next.time) / WEEK_SECONDS);
    const last = Math.floor((end - 1 - next.time) / WEEK_SECONDS);
    for (let k = first; k <= last; k++) {
      const episode = next.episode + k;
      if (episode < 1 || (total > 0 && episode > total)) continue;
      slots.push({
        item,
        episode,
        time: next.time + k * WEEK_SECONDS,
        estimate: k !== 0,
        behind: k === 0 ? Math.max(0, next.episode - 1 - item.entry.watchedEpisodes) : 0,
      });
    }
  }
  return slots.sort((a, b) => a.time - b.time);
}

function byDay(slots: Slot[]): Map<string, Slot[]> {
  const days = new Map<string, Slot[]>();
  for (const slot of slots) {
    const key = dayKey(new Date(slot.time * 1000));
    const list = days.get(key);
    if (list) list.push(slot);
    else days.set(key, [slot]);
  }
  return days;
}

export function SchedulePage() {
  const app = useApp();
  const now = useNow();
  const { data: items = [], isLoading } = useList();
  const { data: settings } = useSettings();
  const openMenu = useAnimeMenu();
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const hideNsfw = settings?.["app.hideNsfw"] ?? true;
  const [scope, setScope] = usePref<Scope>("schedule.scope", "watching");
  const [view, setView] = usePref<View>("schedule.view", "week");
  /** Weeks (or months) away from the current one. */
  const [offset, setOffset] = useState(0);
  const firstDay = useMemo(firstDayOfWeek, []);

  const rows = useMemo(() => {
    const result: Row[] = [];
    for (const item of items) {
      if (scope === "watching" ? !isWatching(item.entry) : !followsAiring(item)) continue;
      if (hideNsfw && isNsfw(item.anime)) continue;
      const next = nextAiring(item);
      if (next) result.push({ item, next });
    }
    return result.sort((a, b) => a.next.time - b.next.time);
  }, [items, scope, hideNsfw]);

  const today = startOfDay(new Date(now * 1000));

  /** The days shown: a week, or the whole weeks covering a month. */
  const range = useMemo(() => {
    if (view === "month") {
      const month = new Date(today.getFullYear(), today.getMonth() + offset, 1);
      const start = startOfWeek(month, firstDay);
      const nextMonth = new Date(month.getFullYear(), month.getMonth() + 1, 1);
      const days: Date[] = [];
      for (let d = start; d < nextMonth || days.length % 7 !== 0; d = addDays(d, 1)) days.push(d);
      return { days, month: month.getMonth(), title: month.toLocaleDateString(undefined, { month: "long", year: "numeric" }) };
    }
    const start = addDays(startOfWeek(today, firstDay), offset * 7);
    const days = Array.from({ length: 7 }, (_, i) => addDays(start, i));
    // "November 23 – 29, 2026", in the locale's own order.
    const title = new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric", year: "numeric" }).formatRange(
      start,
      days[6],
    );
    return { days, month: null, title };
    // `today` is a new object on every render; its time value changes once a day.
  }, [view, offset, firstDay, today.getTime()]);

  const days = useMemo(() => {
    if (view === "list") return new Map<string, Slot[]>();
    const start = seconds(range.days[0]);
    const end = seconds(addDays(range.days[range.days.length - 1], 1));
    return byDay(slotsBetween(rows, start, end));
  }, [rows, range, view]);

  const open = (item: ListItem) => app.openAnime(item.entry.animeId);
  const menu = (e: React.MouseEvent, item: ListItem, title: string) =>
    openMenu(e, { animeId: item.entry.animeId, anime: item.anime, entry: item.entry, title });

  function slotTooltip(slot: Slot, title: string): string {
    const total = slot.item.anime?.episodeCount ?? 0;
    const lines = [
      title,
      `Episode ${slot.episode}${total > 0 ? ` of ${total}` : ""}`,
      new Date(slot.time * 1000).toLocaleString(),
    ];
    if (slot.estimate) lines.push("Estimated: a week apart from the next reported airing");
    if (slot.behind > 0) lines.push(`${slot.behind} aired episode${slot.behind === 1 ? "" : "s"} to catch up on`);
    return lines.join("\n");
  }

  function statusBadge(time: number) {
    const left = countdown(time, now);
    return <span className={left === "aired" ? "badge ok" : "badge"}>{left === "aired" ? "Aired" : `in ${left}`}</span>;
  }

  function renderWeekSlot(slot: Slot) {
    const { item } = slot;
    const title = itemTitle(item, language);
    const aired = slot.time <= now;
    return (
      <li
        key={`${item.entry.animeId}-${slot.episode}`}
        className={`cal-entry${aired ? " aired" : ""}${slot.estimate ? " estimate" : ""}`}
        title={slotTooltip(slot, title)}
        onDoubleClick={() => open(item)}
        onContextMenu={(e) => menu(e, item, title)}
      >
        <Poster anime={item.anime} title={title} className="poster tiny" />
        <span className="cal-entry-text">
          <button className="link cal-entry-title" onClick={() => open(item)}>
            {title}
          </button>
          <span className="muted small">
            {slot.estimate && "~"}
            {timeText(slot.time)} · Ep {slot.episode}
            {slot.behind > 0 && <span className="cal-behind" aria-label={`${slot.behind} to catch up on`}> +{slot.behind}</span>}
          </span>
          {statusBadge(slot.time)}
        </span>
      </li>
    );
  }

  function renderMonthSlot(slot: Slot) {
    const { item } = slot;
    const title = itemTitle(item, language);
    return (
      <li
        key={`${item.entry.animeId}-${slot.episode}`}
        className={`cal-chip${slot.time <= now ? " aired" : ""}${slot.estimate ? " estimate" : ""}`}
        title={slotTooltip(slot, title)}
        onClick={() => open(item)}
        onContextMenu={(e) => menu(e, item, title)}
      >
        <span className="cal-chip-time">{timeText(slot.time)}</span> {title} <span className="muted">{slot.episode}</span>
      </li>
    );
  }

  function renderWeek() {
    const todayKey = dayKey(today);
    return (
      <div className="cal-week">
        {range.days.map((day) => {
          const key = dayKey(day);
          const slots = days.get(key) ?? [];
          const isToday = key === todayKey;
          // On today, a line between what has aired and what hasn't.
          const nowAt = isToday ? slots.findIndex((s) => s.time > now) : -1;
          return (
            <section key={key} className={`cal-day${isToday ? " today" : ""}`} aria-label={day.toDateString()}>
              <header className="cal-day-head">
                <span className="cal-weekday">{day.toLocaleDateString(undefined, { weekday: "short" })}</span>
                <span className="cal-date">{day.getDate()}</span>
              </header>
              <ul className="cal-day-list">
                {slots.map((slot, i) => [
                  i === nowAt && <li key="now" className="cal-now" aria-label="Now" />,
                  renderWeekSlot(slot),
                ])}
                {isToday && slots.length > 0 && nowAt === -1 && <li key="now" className="cal-now" aria-label="Now" />}
              </ul>
            </section>
          );
        })}
      </div>
    );
  }

  function renderMonth() {
    const todayKey = dayKey(today);
    return (
      <div className="cal-month">
        {range.days.slice(0, 7).map((day) => (
          <div key={`h${day.getDay()}`} className="cal-month-weekday">
            {day.toLocaleDateString(undefined, { weekday: "short" })}
          </div>
        ))}
        {range.days.map((day) => {
          const key = dayKey(day);
          const slots = days.get(key) ?? [];
          const outside = range.month !== null && day.getMonth() !== range.month;
          return (
            <section
              key={key}
              className={`cal-cell${key === todayKey ? " today" : ""}${outside ? " outside" : ""}`}
              aria-label={day.toDateString()}
            >
              <span className="cal-date">{day.getDate()}</span>
              <ul className="cal-cell-list">{slots.map(renderMonthSlot)}</ul>
            </section>
          );
        })}
      </div>
    );
  }

  function renderList() {
    const groups: { key: string; label: string; rows: Row[] }[] = [];
    for (const row of rows) {
      // Aired, but the service hasn't reported the next episode yet: today.
      const time = Math.max(row.next.time, now);
      const key = new Date(time * 1000).toDateString();
      const last = groups.at(-1);
      if (last?.key === key) last.rows.push(row);
      else groups.push({ key, label: dayLabel(time, now), rows: [row] });
    }
    return (
      <div className="history">
        {groups.map((group) => (
          <section key={group.key}>
            <h2 className="section-title">{group.label}</h2>
            <ul className="history-list schedule-list">
              {group.rows.map(({ item, next }) => {
                const id = item.entry.animeId;
                const title = itemTitle(item, language);
                const total = item.anime?.episodeCount ?? 0;
                const behind = next.episode - 1 - item.entry.watchedEpisodes;
                const at = new Date(next.time * 1000);
                return (
                  <li key={id} onDoubleClick={() => open(item)} onContextMenu={(e) => menu(e, item, title)}>
                    <span className="history-time muted small" title={at.toLocaleString()}>
                      {timeText(next.time)}
                    </span>
                    <Poster anime={item.anime} title={title} className="poster tiny" />
                    <span className="schedule-title">
                      <button className="link history-title" onClick={() => open(item)}>
                        {title}
                      </button>
                      <span className="muted small">
                        Episode {next.episode}
                        {total > 0 && ` of ${total}`}
                        {behind > 0 && ` · ${behind} aired episode${behind === 1 ? "" : "s"} to catch up on`}
                      </span>
                    </span>
                    {statusBadge(next.time)}
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </div>
    );
  }

  return (
    <div className="page">
      <header className="page-header">
        <h1>Schedule</h1>
        <div className="page-actions">
          <div className="segmented" role="group" aria-label="Show">
            <button aria-pressed={scope === "watching"} onClick={() => setScope("watching")}>
              Watching
            </button>
            <button aria-pressed={scope === "list"} onClick={() => setScope("list")}>
              Also planned and on hold
            </button>
          </div>
        </div>
      </header>

      <div className="toolbar cal-toolbar">
        <div className="segmented" role="group" aria-label="View">
          {(["week", "month", "list"] as const).map((v) => (
            <button
              key={v}
              aria-pressed={view === v}
              onClick={() => {
                setView(v);
                setOffset(0);
              }}
            >
              {v === "week" ? "Week" : v === "month" ? "Month" : "List"}
            </button>
          ))}
        </div>
        {view !== "list" && (
          <>
            <div className="cal-nav">
              <button aria-label={`Previous ${view}`} onClick={() => setOffset(offset - 1)}>
                ‹
              </button>
              <button disabled={offset === 0} onClick={() => setOffset(0)}>
                Today
              </button>
              <button aria-label={`Next ${view}`} onClick={() => setOffset(offset + 1)}>
                ›
              </button>
            </div>
            <h2 className="cal-title">{range.title}</h2>
            <span className="muted small cal-legend" title="Only the next airing comes from your service; the others are a week apart from it">
              ~ estimated
            </span>
          </>
        )}
      </div>

      {isLoading ? (
        <p className="empty">Loading…</p>
      ) : rows.length === 0 ? (
        <p className="empty">
          Nothing here has an upcoming episode. Airing times come from your service and update when you sync.
        </p>
      ) : view === "week" ? (
        renderWeek()
      ) : view === "month" ? (
        renderMonth()
      ) : (
        renderList()
      )}
    </div>
  );
}
