import { useMutation, useQuery } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Fragment, useEffect, useState } from "react";
import {
  api,
  errorCode,
  errorMessage,
  LIST_ACTION_LABELS,
  type AccountStatus,
  type ListAction,
  type Settings,
  type SharingStatus,
} from "../api";
import { Field, LazyInput, Toggle } from "../components/inputs";
import { TorrentsSection } from "./TorrentSettings";
import { useApp } from "../lib/context";
import { usePref } from "../lib/prefs";
import { keys, queryClient, useAccounts, useAppInfo, usePlayers, useSettings, useTaigaDetection, useUpdateSettings } from "../lib/queries";
import { writeText } from "../lib/clipboard";
import { AppUpdates } from "../components/AppUpdates";
import { importFromTaiga } from "../lib/taiga";
import { confirmAction } from "../lib/useAnimeMenu";

const SECTIONS = [
  ["application", "Application"],
  ["accounts", "Accounts"],
  ["library", "Library"],
  ["recognition", "Recognition"],
  ["players", "Media players"],
  ["streaming", "Streaming"],
  ["sharing", "Sharing"],
  ["torrents", "Torrents"],
  ["advanced", "Advanced"],
] as const;

type Section = (typeof SECTIONS)[number][0];

export type Change = <K extends keyof Settings>(key: K, value: Settings[K]) => void;

export function SettingsPage() {
  const [section, setSection] = usePref<Section>("settings.section", "application");
  const { data: settings } = useSettings();
  const update = useUpdateSettings();
  const app = useApp();

  const change: Change = (key, value) =>
    update.mutate((s) => ({ ...s, [key]: value }), {
      onError: (e) => app.notify(errorMessage(e)),
    });

  return (
    <div className="page settings-page">
      <header className="page-header">
        <h1>Settings</h1>
      </header>
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map(([id, label]) => (
            <button
              key={id}
              className={section === id ? "nav-item active" : "nav-item"}
              aria-current={section === id ? "page" : undefined}
              onClick={() => setSection(id)}
            >
              {label}
            </button>
          ))}
        </nav>
        <div className="settings-body">
          {!settings ? (
            <p className="empty">Loading…</p>
          ) : (
            <>
              {section === "application" && <ApplicationSection settings={settings} change={change} />}
              {section === "accounts" && <AccountsSection settings={settings} change={change} />}
              {section === "library" && <LibrarySection settings={settings} change={change} />}
              {section === "recognition" && <RecognitionSection settings={settings} change={change} />}
              {section === "players" && <PlayersSection settings={settings} change={change} />}
              {section === "streaming" && <StreamingSection settings={settings} change={change} />}
              {section === "sharing" && <SharingSection settings={settings} change={change} />}
              {section === "torrents" && <TorrentsSection settings={settings} change={change} />}
              {section === "advanced" && <AdvancedSection settings={settings} change={change} />}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export interface SectionProps {
  settings: Settings;
  change: Change;
}

const KEY_NAMES: Record<string, string> = {
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  " ": "Space",
};

/** `Ctrl+Alt+Shift+Up` from a key press, or null for a lone modifier. */
function accelerator(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">) {
  if (["Control", "Alt", "Shift", "Meta"].includes(e.key)) return null;
  let key = KEY_NAMES[e.key] ?? e.key;
  // Letters and digits by position, so Shift doesn't change them.
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (key.length === 1) key = key.toUpperCase();
  const parts = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super", key];
  return parts.filter(Boolean).join("+");
}

/** Records a shortcut: focus it and press the keys. */
function ShortcutInput({ id, value, onChange }: { id: string; value: string; onChange: (value: string) => void }) {
  return (
    <span className="inline">
      <input
        id={id}
        readOnly
        className="shortcut-input"
        value={value || "Off"}
        onKeyDown={(e) => {
          if (e.key === "Tab") return;
          e.preventDefault();
          if ((e.key === "Backspace" || e.key === "Delete") && !e.ctrlKey && !e.altKey) {
            onChange("");
            return;
          }
          const combo = accelerator(e.nativeEvent);
          // A shortcut needs a modifier, or every press of the key would be taken.
          if (combo && combo.includes("+")) onChange(combo);
        }}
      />
      <button disabled={!value} onClick={() => onChange("")}>
        Turn off
      </button>
    </span>
  );
}

function ShortcutSettings({ settings, change }: SectionProps) {
  const { data: status = [] } = useQuery({
    queryKey: ["hotkeys", settings["app.hotkeys.increment"], settings["app.hotkeys.toggleDetection"]],
    queryFn: api.hotkeys,
  });
  const error = (action: string) => status.find((s) => s.action === action)?.error ?? undefined;
  return (
    <>
      <h3>Keyboard shortcuts</h3>
      <p className="muted small">
        These work anywhere in Windows. Click a box and press the keys; Backspace turns it off. In Patchouli, Ctrl+K
        jumps to any page or anime.
      </p>
      <Field
        label="Next episode watched"
        htmlFor="hotkey-increment"
        hint={error("increment") ?? "Records the next episode of what's playing."}
      >
        <ShortcutInput
          id="hotkey-increment"
          value={settings["app.hotkeys.increment"]}
          onChange={(v) => change("app.hotkeys.increment", v)}
        />
      </Field>
      <Field
        label="Pause detection"
        htmlFor="hotkey-detection"
        hint={error("toggleDetection") ?? "Stops or resumes checking players."}
      >
        <ShortcutInput
          id="hotkey-detection"
          value={settings["app.hotkeys.toggleDetection"]}
          onChange={(v) => change("app.hotkeys.toggleDetection", v)}
        />
      </Field>
    </>
  );
}

function ApplicationSection({ settings, change }: SectionProps) {
  return (
    <section className="settings-section">
      <h2>Application</h2>
      <Toggle
        label="Check for updates automatically"
        description="Checks for app releases and refreshes recognition data."
        checked={settings["app.checkForUpdates"]}
        onChange={(value) => change("app.checkForUpdates", value)}
      />
      <AppUpdates />
      <Field label="Theme" htmlFor="color-scheme">
        <select
          id="color-scheme"
          value={settings["app.colorScheme"]}
          onChange={(e) => change("app.colorScheme", e.target.value as Settings["app.colorScheme"])}
        >
          <option value="system">Same as system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </Field>
      <Field label="Title language" htmlFor="title-language" hint="e.g. Shingeki no Kyojin / Attack on Titan / 進撃の巨人">
        <select
          id="title-language"
          value={settings["app.titleLanguage"]}
          onChange={(e) => change("app.titleLanguage", e.target.value as Settings["app.titleLanguage"])}
        >
          <option value="romaji">Romaji</option>
          <option value="english">English</option>
          <option value="native">Native</option>
        </select>
      </Field>
      {(
        [
          ["app.list.doubleClickAction", "Double-click in the list", "double-click"],
          ["app.list.middleClickAction", "Middle-click in the list", "middle-click"],
        ] as const
      ).map(([key, label, id]) => (
        <Field key={key} label={label} htmlFor={id}>
          <select id={id} value={settings[key]} onChange={(e) => change(key, e.target.value as ListAction)}>
            {(Object.keys(LIST_ACTION_LABELS) as ListAction[]).map((action) => (
              <option key={action} value={action}>
                {LIST_ACTION_LABELS[action]}
              </option>
            ))}
          </select>
        </Field>
      ))}
      <Toggle
        label="Hide adult content"
        checked={settings["app.hideNsfw"]}
        onChange={(v) => change("app.hideNsfw", v)}
      />
      <Toggle
        label="Close to tray"
        description="Closing the window keeps Patchouli running in the notification area."
        checked={settings["app.closeToTray"]}
        onChange={(v) => change("app.closeToTray", v)}
      />
      <Toggle
        label="Minimise to tray"
        description="Minimising hides the window; open it again from the notification area."
        checked={settings["app.minimizeToTray"]}
        onChange={(v) => change("app.minimizeToTray", v)}
      />
      <Toggle
        label="Start in tray"
        description="Start without showing the window."
        checked={settings["app.startInTray"]}
        onChange={(v) => change("app.startInTray", v)}
      />
      <Toggle
        label="Start with Windows"
        description="Start in the tray when you sign in to Windows."
        checked={settings["app.startWithWindows"]}
        onChange={(v) => change("app.startWithWindows", v)}
      />
      <ShortcutSettings settings={settings} change={change} />
    </section>
  );
}

function AccountsSection({ settings, change }: SectionProps) {
  const app = useApp();
  const { data: accounts = [] } = useAccounts();
  const [busy, setBusy] = useState<string | null>(null);
  const refresh = () => {
    for (const key of [keys.accounts, keys.list, keys.queue, keys.history, keys.info]) {
      void queryClient.invalidateQueries({ queryKey: key });
    }
  };

  async function login(account: AccountStatus, mirror = false) {
    setBusy(account.service);
    try {
      if (mirror) {
        await api.login(account.service, false, true);
        app.notify(`Logged in to ${account.name}. Changes to your list are copied there now.`);
        await offerCopyAll(account.name, app);
        return;
      }
      try {
        await api.login(account.service);
      } catch (e) {
        if (errorCode(e) !== "replace-local-data") throw e;
        const ok = await confirmAction(errorMessage(e), `Log in to ${account.name}`, "Replace list");
        if (!ok) return;
        await api.login(account.service, true);
      }
      app.notify(`Logged in to ${account.name}.`);
    } catch (e) {
      app.notify(errorMessage(e));
    } finally {
      setBusy(null);
      refresh();
    }
  }

  async function logout(account: AccountStatus) {
    try {
      await api.logout(account.service);
    } catch (e) {
      app.notify(errorMessage(e));
    }
    refresh();
  }

  return (
    <section className="settings-section">
      <h2>Accounts</h2>
      <Toggle
        label="Sync with the service"
        description="Send list changes automatically and fetch your list on startup."
        checked={settings["sync.enabled"]}
        onChange={(v) => change("sync.enabled", v)}
      />
      <div className="account-list">
        {accounts.map((a) => {
          const inactive = (a.current && !a.connected) || (a.mirror && !a.mirrorConnected);
          const waiting = busy === a.service;
          return (
            <div key={a.service} className="account">
              <div>
                <strong>{a.name}</strong>
                {a.current && <span className="badge accent">main list</span>}
                {a.mirror && <span className="badge">copy</span>}
                <div className="muted small">
                  {!a.configured
                    ? "No client ID configured (see README)."
                    : a.username
                      ? `Logged in as ${a.username}${inactive ? " (session not active)" : ""}${a.mirror ? " · changes are copied here" : ""}`
                      : "Not logged in"}
                </div>
              </div>
              <span className="inline">
                {!a.current && a.configured && !a.mirror && (
                  <button
                    className={a.username ? undefined : "primary"}
                    disabled={busy !== null}
                    onClick={() => (a.username ? change("sync.mirror.enabled", true) : void login(a, true))}
                  >
                    {waiting ? "Waiting for browser…" : "Copy changes here"}
                  </button>
                )}
                {a.mirror && <button onClick={() => change("sync.mirror.enabled", false)}>Stop copying</button>}
                {a.current ? (
                  <button
                    className={a.username ? undefined : "primary"}
                    disabled={busy !== null || !a.configured}
                    onClick={() => void login(a)}
                  >
                    {waiting ? "Waiting for browser…" : a.username ? "Log in again" : "Log in"}
                  </button>
                ) : (
                  <button disabled={busy !== null || !a.configured} onClick={() => void login(a)}>
                    Use as main list…
                  </button>
                )}
                {a.username && <button onClick={() => void logout(a)}>Log out</button>}
              </span>
            </div>
          );
        })}
      </div>
      <p className="muted small">
        Logging in opens the service in your browser. Your list comes from the main service. You can copy every
        change to the other service too; using it as your main list instead replaces your list after asking.
      </p>
      <MirrorPanel />
    </section>
  );
}

/** Asks whether to send the whole list to the mirror once. */
async function offerCopyAll(name: string, app?: ReturnType<typeof useApp>) {
  const ok = await confirmAction(
    `Copy your whole list to ${name} now? For anime in your list, what's on ${name} is replaced with your ` +
      `main list. Anime that are only on ${name} are left alone. Large lists take a while.`,
    `Copy list to ${name}`,
    "Copy list",
  );
  if (!ok) return;
  const count = await api.mirrorCopyAll();
  void queryClient.invalidateQueries({ queryKey: keys.mirror });
  app?.notify(`Copying ${count} entries to ${name}.`);
}

function MirrorPanel() {
  const app = useApp();
  const { data: status } = useQuery({ queryKey: keys.mirror, queryFn: api.mirrorStatus, refetchInterval: 15_000 });
  const dismiss = useMutation({
    mutationFn: api.mirrorDismissFailed,
    onSettled: () => void queryClient.invalidateQueries({ queryKey: keys.mirror }),
  });
  if (!status?.enabled) return null;
  const waiting = status.pending - status.failed.length;
  const SHOWN = 20;

  return (
    <div className="mirror-panel">
      <h3>Copying to {status.name}</h3>
      <p className="muted">
        {waiting > 0
          ? `${waiting} change${waiting === 1 ? "" : "s"} waiting to be copied.`
          : "Everything has been copied."}
        {!status.connected && ` Log in to ${status.name} to send changes.`}
      </p>
      {status.failed.length > 0 && (
        <>
          <p>
            {status.failed.length} couldn't be copied. They're tried again every few minutes until you dismiss them.
          </p>
          <ul className="mirror-failed">
            {status.failed.slice(0, SHOWN).map((f) => (
              <li key={f.animeId}>
                <button className="link" onClick={() => app.openAnime(f.animeId)}>
                  {f.title}
                </button>
                <span className="muted small">{f.error}</span>
              </li>
            ))}
          </ul>
          {status.failed.length > SHOWN && (
            <p className="muted small">…and {status.failed.length - SHOWN} more.</p>
          )}
        </>
      )}
      <span className="inline">
        <button
          disabled={!status.connected}
          onClick={() => void offerCopyAll(status.name, app).catch((e) => app.notify(errorMessage(e)))}
        >
          Copy whole list now…
        </button>
        {status.failed.length > 0 && (
          <button disabled={dismiss.isPending} onClick={() => dismiss.mutate()}>
            Dismiss failed
          </button>
        )}
      </span>
    </div>
  );
}

function LibrarySection({ settings, change }: SectionProps) {
  const app = useApp();
  const folders = settings["library.folders"];

  async function add() {
    try {
      const folder = await openDialog({ directory: true, title: "Select anime folder" });
      if (typeof folder === "string" && !folders.includes(folder)) {
        change("library.folders", [...folders, folder]);
      }
    } catch (e) {
      app.notify(errorMessage(e));
    }
  }

  return (
    <section className="settings-section">
      <h2>Library</h2>
      <p className="muted">
        Folders where your anime are. They're scanned when Patchouli starts and every 30 minutes to find the episodes
        you have.
      </p>
      {folders.length === 0 ? (
        <p className="empty">No folders yet.</p>
      ) : (
        <ul className="folder-list">
          {folders.map((f) => (
            <li key={f}>
              <code>{f}</code>
              <button
                onClick={() =>
                  change(
                    "library.folders",
                    folders.filter((x) => x !== f),
                  )
                }
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <button onClick={() => void add()}>Add folder…</button>

      <h3>Available episodes</h3>
      <Toggle
        label="Watch the folders for changes"
        description="Rescan a few seconds after files are added, renamed or removed."
        checked={settings["library.watchFolders"]}
        onChange={(v) => change("library.watchFolders", v)}
      />
      <Field label="Ignore files smaller than" htmlFor="min-file-size">
        <span className="inline">
          <input
            id="min-file-size"
            type="number"
            min={0}
            max={2048}
            className="narrow"
            value={settings["library.minFileSize"]}
            onChange={(e) => {
              const mb = Number.parseInt(e.target.value, 10);
              if (mb >= 0 && mb <= 2048) change("library.minFileSize", mb);
            }}
          />
          <span className="muted">MB (skips samples and previews)</span>
        </span>
      </Field>
      <Toggle
        label="Highlight anime whose next episode is on disk"
        checked={settings["app.list.highlightNewEpisodes"]}
        onChange={(v) => change("app.list.highlightNewEpisodes", v)}
      />
      <Toggle
        label="Show them first in the list"
        checked={settings["app.list.newEpisodesFirst"]}
        disabled={!settings["app.list.highlightNewEpisodes"]}
        onChange={(v) => change("app.list.newEpisodesFirst", v)}
      />
    </section>
  );
}

/** Marking episodes watched by the player's position. */
function PositionSettings({ settings, change }: SectionProps) {
  const percent = settings["track.update.watchedAt"];
  const port = (key: "track.position.mpcPort" | "track.position.vlcPort") => (text: string) => {
    const value = Number.parseInt(text, 10);
    if (value > 0 && value < 65536) change(key, value);
  };
  return (
    <>
      <h3>Position in the episode</h3>
      <p className="muted small">
        mpv, MPC-HC, MPC-BE and VLC can report how far into the episode you are. Their episodes count as watched at a
        point in playback, so skipping ahead or pausing doesn't confuse the timer. Other players use the timer above.
      </p>
      <Toggle
        label="Use the playback position when the player reports it"
        checked={percent > 0}
        onChange={(v) => change("track.update.watchedAt", v ? 85 : 0)}
      />
      {percent > 0 && (
        <>
          <Field label="Watched at" htmlFor="watched-at" hint="Between 50% and 100% of the episode.">
            <span className="inline">
              <input
                id="watched-at"
                type="number"
                min={50}
                max={100}
                className="narrow"
                value={percent}
                onChange={(e) => {
                  const value = Number.parseInt(e.target.value, 10);
                  if (value >= 50 && value <= 100) change("track.update.watchedAt", value);
                }}
              />
              <span className="muted">%</span>
            </span>
          </Field>
          <Field
            label="mpv pipe"
            htmlFor="mpv-ipc"
            hint="Start mpv with --input-ipc-server=mpvsocket (or add it to mpv.conf without the dashes)."
          >
            <LazyInput
              id="mpv-ipc"
              value={settings["track.position.mpvIpc"]}
              onCommit={(v) => change("track.position.mpvIpc", v.trim() || "mpvsocket")}
            />
          </Field>
          <Field
            label="MPC-HC / MPC-BE port"
            htmlFor="mpc-port"
            hint="Turn on Options › Player › Web Interface › Listen on port."
          >
            <LazyInput
              id="mpc-port"
              type="number"
              value={String(settings["track.position.mpcPort"])}
              onCommit={port("track.position.mpcPort")}
            />
          </Field>
          <Field
            label="VLC port"
            htmlFor="vlc-port"
            hint="Turn on the Web interface under Preferences › All › Interface › Main interfaces, and set a password under Lua."
          >
            <LazyInput
              id="vlc-port"
              type="number"
              value={String(settings["track.position.vlcPort"])}
              onCommit={port("track.position.vlcPort")}
            />
          </Field>
          <Field label="VLC password" htmlFor="vlc-password">
            <LazyInput
              id="vlc-password"
              type="password"
              value={settings["track.position.vlcPassword"]}
              onCommit={(v) => change("track.position.vlcPassword", v)}
            />
          </Field>
        </>
      )}
    </>
  );
}

function RecognitionSection({ settings, change }: SectionProps) {
  const trigger = settings["track.update.trigger"];
  return (
    <section className="settings-section">
      <h2>Recognition</h2>
      <Toggle
        label="Detect media players"
        checked={settings["track.detection.enabled"]}
        onChange={(v) => change("track.detection.enabled", v)}
      />
      <Field label="Update the list" htmlFor="update-trigger">
        <select
          id="update-trigger"
          value={trigger}
          onChange={(e) => change("track.update.trigger", e.target.value as Settings["track.update.trigger"])}
        >
          <option value="afterDelay">After watching for a while</option>
          <option value="onPlayerClose">When the player is closed</option>
        </select>
      </Field>
      {trigger === "afterDelay" && (
        <Field label="Wait" htmlFor="update-delay" hint="Between 10 seconds and 1 hour.">
          <span className="inline">
            <input
              id="update-delay"
              type="number"
              min={10}
              max={3600}
              className="narrow"
              value={settings["track.update.delay"]}
              onChange={(e) => {
                const value = Number.parseInt(e.target.value, 10);
                if (value >= 10 && value <= 3600) change("track.update.delay", value);
              }}
            />
            <span className="muted">seconds</span>
          </span>
        </Field>
      )}
      <Toggle
        label="Pause while the player isn't focused"
        checked={settings["track.update.pauseWhenUnfocused"]}
        onChange={(v) => change("track.update.pauseWhenUnfocused", v)}
      />
      <PositionSettings settings={settings} change={change} />
      <Toggle
        label="Only update for episodes in library folders"
        checked={settings["track.update.libraryOnly"]}
        onChange={(v) => change("track.update.libraryOnly", v)}
      />

      <h3>Notifications</h3>
      <Toggle
        label="When an episode is recognised"
        description="Shows the anime, episode and release group."
        checked={settings["track.notify.recognized"]}
        onChange={(v) => change("track.notify.recognized", v)}
      />
      <Toggle
        label="When something playing isn't recognised"
        checked={settings["track.notify.notRecognized"]}
        onChange={(v) => change("track.notify.notRecognized", v)}
      />
      <Toggle
        label="When a new episode airs"
        description="For anime you're watching. Airing times come from your service."
        checked={settings["track.notify.airing"]}
        onChange={(v) => change("track.notify.airing", v)}
      />

      <h3>Test a file name</h3>
      <RecognitionTester />
    </section>
  );
}

function RecognitionTester() {
  const app = useApp();
  const [input, setInput] = useState("");
  const test = useMutation({ mutationFn: api.testRecognition });
  const result = test.data;
  const id = result?.identification.animeId ?? null;
  const episode = result?.identification.episode.elements.filter((e) => e.kind === "episode") ?? [];

  return (
    <div className="recognition-test">
      <form
        className="inline"
        onSubmit={(e) => {
          e.preventDefault();
          if (input.trim()) test.mutate(input);
        }}
      >
        <input
          type="text"
          className="grow"
          placeholder="[Group] Title - 01 [1080p].mkv, or a full path"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          aria-label="File name to test"
        />
        <button type="submit" disabled={!input.trim() || test.isPending}>
          Test
        </button>
      </form>
      {test.isError && <p className="error">{errorMessage(test.error)}</p>}
      {result && (
        <div className="recognition-result">
          <button
            type="button"
            onClick={() => {
              const filename = test.variables?.split(/[\\/]/).pop();
              const report = JSON.stringify({ filename, ...result }, null, 2);
              void writeText(report).then(() => app.notify("Recognition report copied."));
            }}
          >
            Copy recognition report
          </button>
          <p>
            {id !== null ? (
              <>
                Recognized as{" "}
                <button className="link" onClick={() => app.openAnime(id)}>
                  {result.titles[id] ?? `#${id}`}
                </button>
                {episode.length > 0 && `, episode ${episode.map((e) => e.value).join("–")}`}
              </>
            ) : (
              <strong>Not recognized</strong>
            )}
            {!result.identification.validType && " (openings, endings and previews are ignored)"}
          </p>
          <dl className="facts small">
            <dt>Normalized title</dt>
            <dd>
              <code>{result.identification.normalizedTitle || "—"}</code>
              {result.titleFromPath && " (from the folder name)"}
            </dd>
            {result.elements.map((e, i) => (
              <Fragment key={i}>
                <dt>{e.kind.replace(/_/g, " ")}</dt>
                <dd>{e.value}</dd>
              </Fragment>
            ))}
          </dl>
          {result.identification.candidates.length > 0 && (
            <>
              <h4>Candidates</h4>
              <ul className="small">
                {result.identification.candidates.map((c) => (
                  <li key={c.id}>
                    {result.titles[c.id] ?? `#${c.id}`} (weight {c.weight})
                    {!c.validNumber && " — episode number out of range"}
                    {c.redirection &&
                      ` — redirected to ${result.titles[c.redirection.id] ?? `#${c.redirection.id}`}, episode ${c.redirection.episodes[0]}`}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function PlayersSection({ settings, change }: SectionProps) {
  const { data: players = [], error } = usePlayers();
  const [filter, setFilter] = useState("");
  const disabled = new Set(settings["track.detection.disabledPlayers"]);
  const shown = players.filter((p) => p.toLowerCase().includes(filter.toLowerCase()));

  function toggle(player: string, enabled: boolean) {
    const next = new Set(disabled);
    if (enabled) next.delete(player);
    else next.add(player);
    change("track.detection.disabledPlayers", [...next].sort());
  }

  return (
    <section className="settings-section">
      <h2>Media players</h2>
      <p className="muted">Players to detect. Turn off ones you don't use for anime.</p>
      {error ? (
        <p className="error">{errorMessage(error)}</p>
      ) : (
        <>
          <input
            type="search"
            placeholder={`Filter ${players.length} players`}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            aria-label="Filter players"
          />
          <div className="player-list">
            {shown.map((p) => (
              <Toggle key={p} label={p} checked={!disabled.has(p)} onChange={(v) => toggle(p, v)} />
            ))}
          </div>
        </>
      )}
    </section>
  );
}

function StreamingSection({ settings, change }: SectionProps) {
  return (
    <section className="settings-section">
      <h2>Streaming</h2>
      <Toggle
        label="Detect anime playing in web browsers"
        description="Reads the open tab's title in supported browsers."
        checked={settings["track.detection.streaming.enabled"]}
        onChange={(v) => change("track.detection.streaming.enabled", v)}
      />
    </section>
  );
}

const DISCORD_STATUS: Record<SharingStatus["discord"], string> = {
  disabled: "Off",
  connecting: "Looking for the Discord app…",
  connected: "Connected to Discord",
  invalidClientId: "Discord doesn't recognize the application ID",
};

function SharingSection({ settings, change }: SectionProps) {
  const sharing = settings["sharing.enabled"];
  // Polled, since the connection changes on its own (Discord starting or quitting).
  const { data: status } = useQuery({
    queryKey: ["sharing-status"],
    queryFn: api.sharingStatus,
    refetchInterval: 2000,
    staleTime: 0,
  });
  const discordConfigured = status?.discordConfigured ?? true;
  return (
    <section className="settings-section">
      <h2>Sharing</h2>
      <Toggle
        label="Share what I'm watching"
        description="Anime whose list entry is private are never shared."
        checked={sharing}
        onChange={(v) => change("sharing.enabled", v)}
      />
      <Toggle
        label="Discord status"
        description={
          discordConfigured
            ? "Shows the episode on your Discord profile (needs the Discord app)."
            : "No Discord application ID is configured (TRACKER_DISCORD_APP_ID, see README)."
        }
        checked={settings["sharing.discord.enabled"]}
        disabled={!sharing || !discordConfigured}
        onChange={(v) => change("sharing.discord.enabled", v)}
      />
      {discordConfigured && sharing && settings["sharing.discord.enabled"] && status && (
        <p className={status.discord === "invalidClientId" ? "error small" : "muted small"}>
          {DISCORD_STATUS[status.discord]}
        </p>
      )}
      <Toggle
        label="Webhook"
        description="Sends playing, stopped and updated events as JSON to a URL."
        checked={settings["sharing.webhook.enabled"]}
        disabled={!sharing}
        onChange={(v) => change("sharing.webhook.enabled", v)}
      />
      {settings["sharing.webhook.enabled"] && status?.webhookError && (
        <p className="error small">Last event failed: {status.webhookError}</p>
      )}
      {settings["sharing.webhook.enabled"] && (
        <>
          <Field label="URL" htmlFor="webhook-url">
            <LazyInput
              id="webhook-url"
              type="url"
              placeholder="https://"
              value={settings["sharing.webhook.url"]}
              onCommit={(v) => change("sharing.webhook.url", v.trim())}
            />
          </Field>
          <Field label="Headers" htmlFor="webhook-headers" hint="One “Name: value” per line.">
            <HeadersInput
              value={settings["sharing.webhook.headers"]}
              onCommit={(v) => change("sharing.webhook.headers", v)}
            />
          </Field>
        </>
      )}
    </section>
  );
}

function HeadersInput({ value, onCommit }: { value: string[]; onCommit: (value: string[]) => void }) {
  const [text, setText] = useState(value.join("\n"));
  useEffect(() => setText(value.join("\n")), [value]);
  return (
    <textarea
      id="webhook-headers"
      rows={3}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={() =>
        onCommit(
          text
            .split("\n")
            .map((l) => l.trim())
            .filter(Boolean),
        )
      }
    />
  );
}

function TaigaImport() {
  const app = useApp();
  const { data: taiga } = useTaigaDetection();
  const [busy, setBusy] = useState(false);
  const run = async (path: string | null) => {
    setBusy(true);
    try {
      await importFromTaiga(app, path);
    } finally {
      setBusy(false);
    }
  };
  const browse = async () => {
    const folder = await openDialog({ directory: true, title: "Select Taiga's folder or its data folder" });
    if (typeof folder === "string") await run(folder);
  };
  return (
    <>
      <h3>Import from Taiga</h3>
      <p className="muted small">
        Replaces the local list, history and unsent changes with Taiga's, and copies its settings. Taiga's files
        aren't changed; logins aren't imported.
      </p>
      {taiga?.found.map((f) => (
        <Field key={f.path} label={f.version === "V1" ? "Taiga 1.x" : "Taiga 2"}>
          <span className="inline">
            <code>{f.path}</code>
            <button disabled={busy} onClick={() => void run(f.path)}>
              Import…
            </button>
          </span>
        </Field>
      ))}
      <Field label={taiga?.found.length ? "Other location" : "Location"}>
        <button disabled={busy} onClick={() => void browse()}>
          Choose folder…
        </button>
      </Field>
    </>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function BackupSettings({ settings, change }: SectionProps) {
  const app = useApp();
  const { data: backups = [], refetch } = useQuery({ queryKey: ["backups"], queryFn: api.backups });
  const backUp = useMutation({
    mutationFn: api.backupNow,
    onSuccess: () => app.notify("Backed up."),
    onError: (e) => app.notify(errorMessage(e)),
    onSettled: () => void refetch(),
  });

  async function restore(name: string, time: number) {
    const when = new Date(time * 1000).toLocaleString();
    const ok = await confirmAction(
      `Restore the backup from ${when}? Your list, history, unsent changes and settings go back to how they were ` +
        "then. The current data is backed up first, and Patchouli restarts.",
      "Restore backup",
      "Restore and restart",
    );
    if (!ok) return;
    try {
      await api.restoreBackup(name);
    } catch (e) {
      app.notify(errorMessage(e));
    }
  }

  return (
    <>
      <h3>Backups</h3>
      <p className="muted small">
        Copies of your list database, settings and torrent archive. They hold unsent changes and history that your
        service doesn't have.
      </p>
      <Toggle
        label="Back up once a day"
        checked={settings["app.backup.enabled"]}
        onChange={(v) => change("app.backup.enabled", v)}
      />
      <Field label="Keep" htmlFor="backup-keep">
        <span className="inline">
          <input
            id="backup-keep"
            type="number"
            min={1}
            max={100}
            className="narrow"
            value={settings["app.backup.keep"]}
            onChange={(e) => {
              const value = Number.parseInt(e.target.value, 10);
              if (value >= 1 && value <= 100) change("app.backup.keep", value);
            }}
          />
          <span className="muted">backups</span>
        </span>
      </Field>
      {backups.length > 0 ? (
        <ul className="folder-list backup-list">
          {backups.map((b) => (
            <li key={b.name}>
              <span>
                {new Date(b.time * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}{" "}
                <span className="muted small">{formatSize(b.size)}</span>
              </span>
              <button onClick={() => void restore(b.name, b.time)}>Restore…</button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="empty">No backups yet.</p>
      )}
      <span className="inline">
        <button disabled={backUp.isPending} onClick={() => backUp.mutate()}>
          {backUp.isPending ? "Backing up…" : "Back up now"}
        </button>
        <button onClick={() => void api.openBackupFolder().catch((e) => app.notify(errorMessage(e)))}>
          Open folder
        </button>
      </span>
    </>
  );
}

function AdvancedSection({ settings, change }: SectionProps) {
  const { data: info } = useAppInfo();
  const port = settings["network.proxy.port"];
  return (
    <section className="settings-section">
      <h2>Advanced</h2>
      <Field label="Detection interval" htmlFor="detection-interval">
        <span className="inline">
          <input
            id="detection-interval"
            type="number"
            min={1}
            max={60}
            className="narrow"
            value={Math.round(settings["track.detection.interval"] / 1000)}
            onChange={(e) => {
              const seconds = Number.parseInt(e.target.value, 10);
              if (seconds >= 1 && seconds <= 60) change("track.detection.interval", seconds * 1000);
            }}
          />
          <span className="muted">seconds</span>
        </span>
      </Field>

      <BackupSettings settings={settings} change={change} />

      <h3>Proxy</h3>
      <p className="muted small">
        Used for syncing, logins, torrents, the webhook and updates. Leave the host empty to use the Windows proxy
        settings. Posters always use the Windows proxy.
      </p>
      <Field label="Type" htmlFor="proxy-type">
        <select
          id="proxy-type"
          value={settings["network.proxy.type"]}
          onChange={(e) => change("network.proxy.type", e.target.value as Settings["network.proxy.type"])}
        >
          <option value="http">HTTP</option>
          <option value="socks5">SOCKS5</option>
        </select>
      </Field>
      <Field label="Host" htmlFor="proxy-host">
        <LazyInput id="proxy-host" value={settings["network.proxy.host"]} onCommit={(v) => change("network.proxy.host", v.trim())} />
      </Field>
      <Field label="Port" htmlFor="proxy-port">
        <LazyInput
          id="proxy-port"
          type="number"
          placeholder="Not set"
          value={port > 0 ? String(port) : ""}
          onCommit={(v) => change("network.proxy.port", Number.parseInt(v, 10) || -1)}
        />
      </Field>
      <Field label="Username" htmlFor="proxy-user">
        <LazyInput id="proxy-user" value={settings["network.proxy.username"]} onCommit={(v) => change("network.proxy.username", v)} />
      </Field>
      <Field label="Password" htmlFor="proxy-password">
        <LazyInput
          id="proxy-password"
          type="password"
          value={settings["network.proxy.password"]}
          onCommit={(v) => change("network.proxy.password", v)}
        />
      </Field>

      <TaigaImport />

      {info && (
        <>
          <h3>About</h3>
          <dl className="facts">
            <dt>Version</dt>
            <dd>
              {info.name} {info.version}
            </dd>
            <dt>Data folder</dt>
            <dd>
              <code>{info.dataDir}</code>
              {info.portable && " (portable)"}
            </dd>
            <dt>Stored</dt>
            <dd>
              {info.animeCount} anime, {info.listCount} entries, {info.historyCount} history items
            </dd>
          </dl>
        </>
      )}
    </section>
  );
}
