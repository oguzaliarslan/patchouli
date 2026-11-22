import { useMutation } from "@tanstack/react-query";
import { api, errorMessage, type UpdateReason, type UpdateState } from "../api";
import { useApp } from "../lib/context";
import { preferredTitle } from "../lib/format";
import { keys, queryClient, useAnime, useSettings, useTracking } from "../lib/queries";
import { Poster } from "./Poster";

const REASONS: Record<UpdateReason, string> = {
  none: "",
  invalidEpisode: "The episode number doesn't fit this anime.",
  outsideLibrary: "The file isn't in your library folders.",
  alreadyWatched: "You've already watched this episode.",
  skipsAhead: "This episode skips ahead.",
};

function formatRemaining(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return m > 0 ? `${m}:${String(s).padStart(2, "0")}` : `${s} s`;
}

/** What the player is playing and what will happen to the list. */
export function NowPlayingCard() {
  const app = useApp();
  const { data: tracking } = useTracking();
  const { data: settings } = useSettings();
  const playing = tracking?.nowPlaying ?? null;
  const update = tracking?.update;
  const { data: details } = useAnime(playing?.animeId ?? null);
  const language = settings?.["app.titleLanguage"] ?? "romaji";
  const detectionOn = settings?.["track.detection.enabled"] ?? true;

  const act = useMutation({
    mutationFn: (accept: boolean) => (accept ? api.acceptUpdate() : api.cancelUpdate()),
    onSuccess: (state) =>
      queryClient.setQueryData(keys.tracking, (old: typeof tracking) => (old ? { ...old, update: state } : old)),
    onError: (e) => app.notify(errorMessage(e)),
  });

  if (!playing) {
    return (
      <section className="now-playing" aria-label="Now playing">
        <div className="now-playing-icon">▶</div>
        <div>
          <strong>Nothing is playing</strong>
          <p className="muted">
            {detectionOn
              ? "Play an episode in a supported media player and it will show up here."
              : "Media player detection is off (Settings › Recognition)."}
          </p>
        </div>
      </section>
    );
  }

  const episode = playing.episodes.length ? playing.episodes.join("–") : null;
  if (playing.animeId === null) {
    return (
      <section className="now-playing" aria-label="Now playing">
        <div className="now-playing-icon">?</div>
        <div>
          <strong>{playing.title || "Unknown title"}</strong>
          {episode && <span className="muted"> · Episode {episode}</span>}
          <p className="muted">
            Playing in {playing.player}, but it isn't recognized as an anime in your list.{" "}
            <button className="link" onClick={() => app.navigate("settings")}>
              Test the file name
            </button>
          </p>
        </div>
      </section>
    );
  }

  const title = preferredTitle(details?.anime, language, details?.settings?.displayTitle, playing.animeId);
  return (
    <section className="now-playing" aria-label="Now playing">
      <Poster anime={details?.anime} title={title} className="poster small" />
      <div className="now-playing-main">
        <button className="link now-playing-title" onClick={() => app.openAnime(playing.animeId!)}>
          {title}
        </button>
        <div className="muted small">
          {episode ? `Episode ${episode}` : "No episode number"} · {playing.player}
        </div>
        {update && <UpdateStatus update={update} busy={act.isPending} onAnswer={(yes) => act.mutate(yes)} />}
      </div>
    </section>
  );
}

function UpdateStatus({
  update,
  busy,
  onAnswer,
}: {
  update: UpdateState;
  busy: boolean;
  onAnswer: (accept: boolean) => void;
}) {
  switch (update.phase) {
    case "countdown":
      return (
        <p className="update-status">
          {update.byPosition ? (
            <>
              {update.paused ? "Paused. " : ""}
              At {update.positionPercent}%; episode {update.episode} counts as watched at {update.watchedAtPercent}%
              ({formatRemaining(update.remainingSecs)} of playback left).{" "}
            </>
          ) : (
            <>
              {update.paused ? "Paused while the player isn't focused. " : ""}
              Updating to episode {update.episode} in {formatRemaining(update.remainingSecs)}.{" "}
            </>
          )}
          <button className="link" disabled={busy} onClick={() => onAnswer(false)}>
            Don't update
          </button>
        </p>
      );
    case "waitingForClose":
      return (
        <p className="update-status">
          Episode {update.episode} will be recorded when you close the player.{" "}
          <button className="link" disabled={busy} onClick={() => onAnswer(true)}>
            Update now
          </button>{" "}
          <button className="link" disabled={busy} onClick={() => onAnswer(false)}>
            Don't update
          </button>
        </p>
      );
    case "confirming":
      return (
        <div className="update-status confirm">
          <span>
            {REASONS[update.reason]} You've watched {update.previousEpisode}. Record episode{" "}
            {update.episode} anyway?
          </span>
          <span className="inline">
            <button className="primary" disabled={busy} onClick={() => onAnswer(true)}>
              Update
            </button>
            <button disabled={busy} onClick={() => onAnswer(false)}>
              Not now
            </button>
          </span>
        </div>
      );
    case "committed":
      return <p className="update-status success">Your list was updated to episode {update.episode}.</p>;
    case "cancelled":
      return <p className="update-status muted">Not updating for this episode.</p>;
    case "denied":
      return <p className="update-status muted">{REASONS[update.reason]}</p>;
    default:
      return null;
  }
}
