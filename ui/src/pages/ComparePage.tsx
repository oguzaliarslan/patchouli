import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import {
  api,
  errorCode,
  errorMessage,
  type CompareKind,
  type CompareRow,
  type CompareSide,
  type CompareTarget,
  type Comparison,
} from "../api";
import { useApp } from "../lib/context";
import { LIST_STATUS_LABELS, formatDate, formatScore, isEmptyDate, relativeTime } from "../lib/format";
import { keys, queryClient, useCurrentAccount } from "../lib/queries";
import { confirmAction } from "../lib/useAnimeMenu";

const SHOWN = 300;

function entryText(side: CompareSide | null, fields: string[], scoreFormat: string | null) {
  const entry = side?.entry;
  if (!entry) return <span className="muted">Not in list</span>;
  const mark = (field: string, text: string) => (
    <span key={field} className={fields.includes(field) ? "compare-diff" : undefined}>
      {text}
    </span>
  );
  const parts = [
    mark("status", entry.rewatching ? "Rewatching" : LIST_STATUS_LABELS[entry.status]),
    mark("progress", `ep ${entry.watchedEpisodes}`),
    mark("score", entry.score ? formatScore(entry.score, scoreFormat) : "no score"),
  ];
  if (fields.includes("rewatches")) parts.push(mark("rewatches", `rewatched ${entry.rewatchedTimes}×`));
  if (fields.includes("started"))
    parts.push(mark("started", `started ${isEmptyDate(entry.dateStarted) ? "—" : formatDate(entry.dateStarted)}`));
  if (fields.includes("completed"))
    parts.push(
      mark("completed", `completed ${isEmptyDate(entry.dateCompleted) ? "—" : formatDate(entry.dateCompleted)}`),
    );
  return parts.flatMap((p, i) => (i ? [<span key={`sep${i}`}> · </span>, p] : [p]));
}

export function ComparePage() {
  const app = useApp();
  const { scoreFormat } = useCurrentAccount();
  const [tab, setTab] = useState<CompareKind>("different");
  const { data: last } = useQuery({ queryKey: ["compare"], queryFn: api.compareLast });
  const [result, setResult] = useState<Comparison | null>(null);
  const comparison = result ?? last ?? null;

  const compare = useMutation({
    mutationFn: api.compareLists,
    onSuccess: (c) => setResult(c),
  });
  const apply = useMutation({
    mutationFn: ({ rows, target }: { rows: CompareRow[]; target: CompareTarget }) =>
      api.compareApply(
        rows.map((r) => r.key),
        target,
      ),
    onSuccess: (r) => {
      setResult(r.comparison);
      void queryClient.invalidateQueries({ queryKey: keys.list });
      const failed = r.errors.length ? ` ${r.errors.length} failed: ${r.errors[0]}` : "";
      app.notify(`Copied ${r.copied}.${failed}`);
    },
    onError: (e) => app.notify(errorMessage(e)),
  });

  const notLoggedIn = compare.error && errorCode(compare.error) === "other-not-logged-in";
  const main = comparison?.mainName ?? "Main list";
  const other = comparison?.otherName ?? "the other service";
  const otherFormat = comparison?.otherService === "myanimelist" ? "POINT_10" : scoreFormat;

  const counts: Record<CompareKind, number> = { different: 0, onlyMain: 0, onlyOther: 0, noMatch: 0, same: 0 };
  for (const row of comparison?.rows ?? []) counts[row.kind] += 1;
  const rows = (comparison?.rows ?? []).filter((r) => r.kind === tab);

  const TABS: [CompareKind, string][] = [
    ["different", "Different"],
    ["onlyMain", `Only on ${main}`],
    ["onlyOther", `Only on ${other}`],
    ["noMatch", "No match"],
    ["same", "Same"],
  ];

  async function run(rowsToCopy: CompareRow[], target: CompareTarget, confirmText?: string) {
    if (confirmText && !(await confirmAction(confirmText, "Copy entries", "Copy"))) return;
    apply.mutate({ rows: rowsToCopy, target });
  }

  async function removeFrom(row: CompareRow, target: CompareTarget) {
    const name = target === "main" ? main : other;
    const title = (row.main ?? row.other)?.title ?? "this anime";
    if (await confirmAction(`Remove ${title} from your ${name} list?`, "Remove from list", "Remove")) {
      apply.mutate({ rows: [row], target });
    }
  }

  function actions(row: CompareRow) {
    const busy = apply.isPending;
    switch (row.kind) {
      case "different":
        return (
          <>
            <button disabled={busy} onClick={() => void run([row], "other")}>
              Use {main}'s
            </button>
            <button disabled={busy} onClick={() => void run([row], "main")}>
              Use {other}'s
            </button>
          </>
        );
      case "onlyMain":
        return (
          <>
            <button disabled={busy} onClick={() => void run([row], "other")}>
              Add to {other}
            </button>
            <button disabled={busy} onClick={() => void removeFrom(row, "main")}>
              Remove from {main}
            </button>
          </>
        );
      case "onlyOther":
        return (
          <>
            <button disabled={busy} onClick={() => void run([row], "main")}>
              Add to {main}
            </button>
            <button disabled={busy} onClick={() => void removeFrom(row, "other")}>
              Remove from {other}
            </button>
          </>
        );
      default:
        return null;
    }
  }

  function bulk() {
    if (rows.length === 0 || apply.isPending) return null;
    const n = rows.length;
    switch (tab) {
      case "different":
        return (
          <span className="inline">
            <button
              onClick={() =>
                void run(rows, "other", `Make ${n} entries on ${other} match ${main}? What's on ${other} is replaced.`)
              }
            >
              Copy all to {other}
            </button>
            <button
              onClick={() =>
                void run(rows, "main", `Make ${n} entries on ${main} match ${other}? What's on ${main} is replaced.`)
              }
            >
              Copy all to {main}
            </button>
          </span>
        );
      case "onlyMain":
        return (
          <button onClick={() => void run(rows, "other", `Add ${n} anime to your ${other} list?`)}>
            Add all to {other}
          </button>
        );
      case "onlyOther":
        return (
          <button onClick={() => void run(rows, "main", `Add ${n} anime to your ${main} list?`)}>
            Add all to {main}
          </button>
        );
      default:
        return null;
    }
  }

  return (
    <div className="page compare-page">
      <header className="page-header">
        <h1>Compare lists</h1>
        <div className="page-actions">
          <button className="primary" disabled={compare.isPending} onClick={() => compare.mutate()}>
            {compare.isPending ? "Fetching both lists…" : comparison ? "Compare again" : "Compare now"}
          </button>
        </div>
      </header>

      {notLoggedIn ? (
        <section className="callout">
          <p>
            {errorMessage(compare.error)}{" "}
            <button className="link" onClick={() => app.navigate("settings")}>
              Go to Settings
            </button>
          </p>
        </section>
      ) : compare.error ? (
        <p className="error">{errorMessage(compare.error)}</p>
      ) : null}

      {!comparison ? (
        <p className="empty">
          Fetches your AniList and MyAnimeList lists and shows where they differ. You need to be logged in to both.
        </p>
      ) : (
        <>
          <p className="muted small">
            {main} is your main list. Compared {relativeTime(comparison.time)}; changes made since then aren't shown
            until you compare again. Scores are compared in whole points out of 10.
          </p>
          <div className="tabs" role="tablist">
            {TABS.map(([kind, label]) => (
              <button key={kind} role="tab" aria-selected={tab === kind} onClick={() => setTab(kind)}>
                {label}
                <span className="count">{counts[kind]}</span>
              </button>
            ))}
          </div>

          {tab === "noMatch" && (
            <p className="muted small">
              These anime have no counterpart that AniList knows of on the other service, so they can't be copied.
            </p>
          )}

          {rows.length === 0 ? (
            <p className="empty">{tab === "different" ? "No differences." : "None."}</p>
          ) : (
            <>
              <div className="toolbar">{bulk()}</div>
              <ul className="compare-list">
                <li className="compare-head" aria-hidden="true">
                  <span>Anime</span>
                  <span>{main}</span>
                  <span>{other}</span>
                  <span />
                </li>
                {rows.slice(0, SHOWN).map((row) => {
                  const title = (row.main ?? row.other)?.title ?? row.key;
                  const inList = row.main?.entry != null;
                  return (
                    <li key={row.key}>
                      {inList ? (
                        <button className="link compare-title" onClick={() => app.openAnime(row.main!.animeId)}>
                          {title}
                        </button>
                      ) : (
                        <span className="compare-title">{title}</span>
                      )}
                      <span className="small">{entryText(row.main, row.fields, scoreFormat)}</span>
                      <span className="small">{entryText(row.other, row.fields, otherFormat)}</span>
                      <span className="compare-actions">{actions(row)}</span>
                    </li>
                  );
                })}
              </ul>
              {rows.length > SHOWN && <p className="muted small">…and {rows.length - SHOWN} more.</p>}
            </>
          )}
        </>
      )}
    </div>
  );
}
