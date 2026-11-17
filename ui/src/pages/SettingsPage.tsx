import { useMutation, useQuery } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Fragment, useEffect, useState } from "react";
import {
  api,
  errorCode,
  errorMessage,
  type AccountStatus,
  type Settings,
  type SharingStatus,
} from "../api";
import { Field, LazyInput, Toggle } from "../components/inputs";
import { TorrentsSection } from "./TorrentSettings";
import { useApp } from "../lib/context";
import { usePref } from "../lib/prefs";
import { keys, queryClient, useAccounts, useAppInfo, usePlayers, useSettings, useTaigaDetection, useUpdateSettings } from "../lib/queries";
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

function ApplicationSection({ settings, change }: SectionProps) {
  return (
    <section className="settings-section">
      <h2>Application</h2>
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
      <Toggle
        label="Hide adult content"
        checked={settings["app.hideNsfw"]}
        onChange={(v) => change("app.hideNsfw", v)}
      />
      <Toggle
        label="Close to tray"
        description="Closing the window keeps Tracker running in the notification area."
        checked={settings["app.closeToTray"]}
        onChange={(v) => change("app.closeToTray", v)}
      />
      <Toggle
        label="Start in tray"
        description="Start without showing the window."
        checked={settings["app.startInTray"]}
        onChange={(v) => change("app.startInTray", v)}
      />
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

  async function login(account: AccountStatus) {
    setBusy(account.service);
    try {
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
        {accounts.map((a) => (
          <div key={a.service} className="account">
            <div>
              <strong>{a.name}</strong>
              {a.current && <span className="badge accent">current</span>}
              <div className="muted small">
                {!a.configured
                  ? "No client ID configured (see README)."
                  : a.username
                    ? `Logged in as ${a.username}${a.current && !a.connected ? " (session not active)" : ""}`
                    : "Not logged in"}
              </div>
            </div>
            {a.username ? (
              <span className="inline">
                <button disabled={busy !== null || !a.configured} onClick={() => void login(a)}>
                  Log in again
                </button>
                <button onClick={() => void logout(a)}>Log out</button>
              </span>
            ) : (
              <button className="primary" disabled={!a.configured || busy !== null} onClick={() => void login(a)}>
                {busy === a.service ? "Waiting for browser…" : "Log in"}
              </button>
            )}
          </div>
        ))}
      </div>
      <p className="muted small">
        Logging in opens the service in your browser. Your list belongs to one service at a time;
        logging in to the other one replaces it after asking.
      </p>
    </section>
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
      <p className="muted">Folders where your anime are. They're scanned to find episodes (phase 4).</p>
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
    </section>
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
      <Toggle
        label="Only update for episodes in library folders"
        checked={settings["track.update.libraryOnly"]}
        onChange={(v) => change("track.update.libraryOnly", v)}
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

      <h3>Proxy</h3>
      <p className="muted small">Saved now; used for requests once proxy support is added.</p>
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
