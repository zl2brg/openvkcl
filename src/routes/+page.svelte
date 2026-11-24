<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import {
    api,
    emptyStation,
    errorText,
    formatFreq,
    pad,
    dayBound,
    rangeTest,
    RIG_STATUS_EVENT,
    CLUSTER_SPOT_EVENT,
    CLUSTER_STATUS_EVENT,
    type CheckResult,
    type ContestDef,
    type DateRange,
    type LogSetup,
    type LogView,
    type Qso,
    type QsoInput,
    type QrzConfig,
    type QsoRow,
    type RigStatus,
    type Settings,
    type ClusterConfig,
    type ClusterStatus,
    type Spot,
  } from "$lib/api";
  import EntryLine from "$lib/components/EntryLine.svelte";
  import LogTable from "$lib/components/LogTable.svelte";
  import ScorePanel from "$lib/components/ScorePanel.svelte";
  import SetupDialog from "$lib/components/SetupDialog.svelte";
  import RigDialog from "$lib/components/RigDialog.svelte";
  import EditQsoDialog from "$lib/components/EditQsoDialog.svelte";
  import CabrilloDialog from "$lib/components/CabrilloDialog.svelte";
  import AboutDialog from "$lib/components/AboutDialog.svelte";
  import { setTheme, theme } from "$lib/theme.svelte";
  import LookupDialog from "$lib/components/LookupDialog.svelte";
  import AdifDialog from "$lib/components/AdifDialog.svelte";
  import Clock from "$lib/components/Clock.svelte";

  const LOG_FILTER = [{ name: "OpenVKCL log", extensions: ["ovkcl"] }];

  let contests = $state<ContestDef[]>([]);
  let contestErrors = $state<string[]>([]);
  let settings = $state<Settings | null>(null);
  // Raw: the log is only ever replaced, never changed in place, so Svelte
  // needn't track each of what can be tens of thousands of rows.
  let view = $state.raw<LogView | null>(null);
  let logVersion = $state(0);
  let rig = $state<RigStatus>({ connected: false, freq_hz: null, band: null, mode: null, rig_mode: null, error: null });
  let clusterStatus = $state<ClusterStatus>({ connected: false, node: "", error: null });
  /** Recent DX cluster spots, oldest first: the last two hours, as the backend keeps. */
  let spots = $state<Spot[]>([]);
  const KEEP_SPOTS = 5000;
  const KEEP_SPOTS_MS = 2 * 3_600_000;
  let version = $state("");
  let notice = $state("");
  let noticeIsError = $state(false);
  let check = $state<CheckResult | null>(null);
  /** The call being typed on the entry line: the log shows only its contacts. */
  let typedCall = $state("");

  let setupDialog = $state<{ initial: LogSetup; isNew: boolean } | null>(null);
  let rigDialog = $state(false);
  let aboutDialog = $state(false);
  let lookupDialog = $state(false);
  let editRow = $state<QsoRow | null>(null);
  let cabrillo = $state<string | null>(null);
  let adifDialog = $state<{ kind: "import" | "export"; file?: string } | null>(null);
  /** Dates the log is shown between, in general logs. */
  let range = $state<DateRange>({ from: "", to: "" });

  let entry: EntryLine | undefined = $state();

  let fileName = $derived(view?.path.split(/[\\/]/).pop() ?? "");
  let worked = $derived(new Set(view?.qsos.map((q) => q.call) ?? []));
  let general = $derived(!!view?.contest.general);
  let rangeRows = $derived.by(() => {
    if (!view || !general) return [];
    const inRange = rangeTest(range);
    return view.qsos.filter((q) => inRange(q.time));
  });

  function say(text: string, isError = false) {
    notice = text;
    noticeIsError = isError;
  }

  function setView(v: LogView) {
    if (v.path !== view?.path) range = { from: "", to: "" };
    view = v;
    logVersion++;
  }

  async function refocus() {
    await tick();
    entry?.focusCall();
  }

  onMount(() => {
    const unlisten = listen<RigStatus>(RIG_STATUS_EVENT, (e) => (rig = e.payload));
    const unlistenSpots = listen<Spot>(CLUSTER_SPOT_EVENT, (e) => {
      const cutoff = Date.now() - KEEP_SPOTS_MS;
      const kept = spots.length && new Date(spots[0].time).getTime() < cutoff
        ? spots.filter((s) => new Date(s.time).getTime() >= cutoff)
        : spots;
      spots = [...kept.slice(-(KEEP_SPOTS - 1)), e.payload];
    });
    const unlistenCluster = listen<ClusterStatus>(CLUSTER_STATUS_EVENT, (e) => (clusterStatus = e.payload));
    (async () => {
      try {
        const list = await api.listContests();
        contests = list.contests;
        contestErrors = list.errors;
        settings = await api.getSettings();
        version = await api.appVersion();
        rig = await api.rigStatus();
        clusterStatus = await api.clusterStatus();
        spots = await api.clusterSpots();
        const current = await api.getLog();
        if (current) {
          setView(current);
        } else if (settings.last_log) {
          try {
            setView(await api.openLog(settings.last_log));
          } catch {
            // The last log may have been moved or deleted; start with no log open.
          }
        }
        refocus();
      } catch (e) {
        say(errorText(e), true);
      }
      refreshLookupFiles();
    })();
    return () => {
      unlisten.then((f) => f());
      unlistenSpots.then((f) => f());
      unlistenCluster.then((f) => f());
    };
  });

  /** Lookup files older than this are downloaded again at startup. */
  const LOOKUP_MAX_AGE_MS = 7 * 24 * 3_600_000;

  type FileStatus = { count: number; updated: string | null };

  const stale = (s: FileStatus) =>
    !s.count || !s.updated || Date.now() - new Date(s.updated).getTime() > LOOKUP_MAX_AGE_MS;

  /** Download the country file and MASTER.SCP in the background when missing or a week old. */
  async function refreshLookupFiles() {
    const done: string[] = [];
    const failed: string[] = [];
    const update = async (name: string, status: () => Promise<FileStatus>, download: () => Promise<unknown>) => {
      try {
        if (!stale(await status())) return;
        await download();
        done.push(name);
      } catch {
        failed.push(name);
      }
    };
    await Promise.all([
      update("country file", api.ctyStatus, api.ctyDownload),
      update("MASTER.SCP", api.scpStatus, api.scpDownload),
    ]);
    if (done.includes("country file")) {
      const v = await api.getLog().catch(() => null);
      if (v) setView(v);
    }
    if (failed.length) say(`Couldn't download the ${failed.join(" or ")}: try again from Lookups…`, true);
    // Don't cover a message about the log itself.
    else if (done.length && !notice) say(`Updated the ${done.join(" and ")}`);
  }

  function newLogSetup(): LogSetup {
    const contest = contests[0];
    const today = new Date();
    const date = `${today.getUTCFullYear()}-${pad(today.getUTCMonth() + 1)}-${pad(today.getUTCDate())}`;
    return {
      contest_id: contest?.id ?? "",
      start: new Date(`${date}T${contest?.default_start_utc ?? "00:00"}:00Z`).toISOString(),
      station: settings ? { ...settings.station } : emptyStation(),
      sent_number: "",
      serial_per_band: contest?.serial_per_band ?? false,
      utc_offset_minutes: -today.getTimezoneOffset(),
      location: "",
      categories: Object.fromEntries((contest?.categories ?? []).map((c) => [c.key, c.default])),
      bonus_bands: {},
      soapbox: "",
    };
  }

  function withExt(path: string, ext: string) {
    return path.toLowerCase().endsWith(`.${ext}`) ? path : `${path}.${ext}`;
  }

  async function createLog(setup: LogSetup) {
    const year = new Date(setup.start).getUTCFullYear();
    const call = setup.station.callsign.replace(/\//g, "_");
    const isGeneral = contests.find((c) => c.id === setup.contest_id)?.general;
    const path = await save({
      title: "Save new log as",
      defaultPath: isGeneral ? `logbook-${call}.ovkcl` : `${setup.contest_id}-${year}-${call}.ovkcl`,
      filters: LOG_FILTER,
    });
    if (!path) return;
    try {
      setView(await api.newLog(withExt(path, "ovkcl"), setup));
      setupDialog = null;
      if (settings) {
        settings.station = { ...setup.station };
        await api.saveSettings($state.snapshot(settings) as Settings);
      }
      say(`Created ${withExt(path, "ovkcl")}`);
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function saveSetup(setup: LogSetup) {
    if (setupDialog?.isNew) return createLog(setup);
    try {
      setView(await api.updateSetup(setup));
      setupDialog = null;
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function openLog() {
    const path = await open({ title: "Open log", filters: LOG_FILTER, multiple: false, directory: false });
    if (!path) return;
    try {
      setView(await api.openLog(path));
      say(`Opened ${path}`);
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function logQso(input: QsoInput) {
    const logged = await api.logQso(input);
    if (logged.kind === "view") {
      setView(logged.view);
    } else if (view) {
      // A general log sends just the new contact, the newest in the log.
      setView({ ...view, qsos: [...view.qsos, logged.row] });
    }
  }

  async function saveQso(qso: Qso) {
    try {
      setView(await api.updateQso(qso));
      editRow = null;
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function deleteQso(id: number) {
    try {
      setView(await api.deleteQso(id));
      editRow = null;
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function showCabrillo() {
    try {
      cabrillo = await api.cabrilloText();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function saveCabrillo() {
    if (!view) return;
    const path = await save({
      title: "Save Cabrillo log",
      defaultPath: exportName("log"),
      filters: [{ name: "Cabrillo log", extensions: ["log", "cbr", "txt"] }],
    });
    if (!path) return;
    try {
      await api.exportCabrillo(path);
      cabrillo = null;
      say(`Saved Cabrillo log to ${path}`);
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  /** The file name for exports: the callsign, and the contest for contest logs. */
  function exportName(ext: string) {
    if (!view) return "";
    const call = view.setup.station.callsign.replace(/\//g, "_");
    return general ? `${call}-log.${ext}` : `${call}-${view.contest.id}.${ext}`;
  }

  async function exportAdif(r: DateRange): Promise<boolean> {
    const path = await save({
      title: "Export ADIF",
      defaultPath: exportName("adi"),
      filters: [{ name: "ADIF", extensions: ["adi"] }],
    });
    if (!path) return false;
    const n = await api.exportAdif(withExt(path, "adi"), dayBound(r.from), dayBound(r.to, true));
    say(`Exported ${n.toLocaleString()} contacts to ${withExt(path, "adi")}`);
    return true;
  }

  async function chooseAdifImport() {
    const file = await open({
      title: "Import ADIF",
      filters: [{ name: "ADIF", extensions: ["adi", "adif", "ADI", "txt"] }],
      multiple: false,
      directory: false,
    });
    if (file) adifDialog = { kind: "import", file };
  }

  async function importAdif(r: DateRange) {
    const result = await api.importAdif(adifDialog?.file ?? "", dayBound(r.from), dayBound(r.to, true));
    setView(result.view);
    say(`Imported ${result.summary.imported.toLocaleString()} contacts`);
    return result.summary;
  }

  async function exportScoreSheet() {
    if (!view) return;
    const path = await save({
      title: "Export score sheet",
      defaultPath: exportName("csv"),
      filters: [{ name: "CSV spreadsheet", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportScoreSheet(withExt(path, "csv"));
      say(`Exported score sheet to ${withExt(path, "csv")}`);
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function saveLookups(qrz: QrzConfig, cluster: ClusterConfig) {
    if (!settings) return;
    try {
      settings.qrz = qrz;
      settings.cluster = cluster;
      await api.saveSettings($state.snapshot(settings) as Settings);
      clusterStatus = await api.clusterConnect(cluster);
      lookupDialog = false;
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }

  async function connectRig(config: Settings["rig"]) {
    try {
      await api.rigConnect(config);
      if (settings) settings.rig = config;
      rigDialog = false;
      refocus();
    } catch (e) {
      say(errorText(e), true);
    }
  }
</script>

<div class="app">
  <header>
    <div class="title">
      <strong>OpenVKCL</strong>
      {#if view}
        <span>{view.contest.name}</span>
        <span class="mono call">{view.setup.station.callsign}</span>
        <span class="muted" title={view.path}>{fileName}</span>
      {/if}
    </div>
    <div class="rig" class:on={rig.connected}>
      <span class="dot"></span>
      {#if rig.connected && rig.freq_hz == null}
        <span>Connected</span>
        <span class="muted" title="Open a slice in SmartSDR to follow it">no slice active</span>
      {:else if rig.connected}
        <span class="mono">{formatFreq(rig.freq_hz)} kHz</span>
        <span>{rig.rig_mode ?? ""}</span>
        <span class="muted">{rig.band ?? "out of band"}</span>
      {:else}
        <span class="muted" title={rig.error ?? ""}>{rig.error ? "Radio not responding" : "No radio"}</span>
      {/if}
    </div>
    <Clock />
    <button
      class="theme-toggle"
      onclick={() => setTheme(theme.mode === "dark" ? "light" : "dark")}
      title="Switch to {theme.mode === 'dark' ? 'light' : 'dark'} mode"
      aria-label="Switch to {theme.mode === 'dark' ? 'light' : 'dark'} mode"
    >
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        {#if theme.mode === "dark"}
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
        {:else}
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
        {/if}
      </svg>
    </button>
  </header>

  <nav>
    <button onclick={() => (setupDialog = { initial: newLogSetup(), isNew: true })} disabled={!contests.length}>New log</button>
    <button onclick={openLog}>Open log</button>
    <button onclick={() => view && (setupDialog = { initial: view.setup, isNew: false })} disabled={!view}>Setup</button>
    <span class="sep"></span>
    {#if !general}
      <button onclick={showCabrillo} disabled={!view}>Cabrillo…</button>
    {/if}
    <button onclick={chooseAdifImport} disabled={!view} title="Add contacts from an ADIF file to this log">Import ADIF…</button>
    <button onclick={() => (adifDialog = { kind: "export" })} disabled={!view}>Export ADIF…</button>
    {#if !general}
      <button onclick={exportScoreSheet} disabled={!view} title="Per-band sheet with distance and points for each contact">
        Score sheet
      </button>
    {/if}
    <span class="sep"></span>
    <button onclick={() => (rigDialog = true)}>Radio…</button>
    <button onclick={() => (lookupDialog = true)} title="Super Check Partial and QRZ.com">Lookups…</button>
    <button onclick={() => (aboutDialog = true)}>About</button>
    {#if notice}
      <span class="notice" class:error={noticeIsError}>{notice}</span>
    {/if}
  </nav>

  {#if view}
    <main>
      <div class="left">
        <EntryLine
          bind:this={entry}
          bind:check
          bind:typed={typedCall}
          contest={view.contest}
          setup={view.setup}
          {rig}
          {logVersion}
          {worked}
          qrzEnabled={!!settings?.qrz.enabled}
          onlog={logQso}
        />
        <div class="table">
          <LogTable
            rows={view.qsos}
            contest={view.contest}
            highlight={check?.previous[0]?.call ?? ""}
            callFilter={typedCall}
            bind:range
            onedit={(r) => (editRow = r)}
            ondelete={(r) => deleteQso(r.id)}
            ondone={refocus}
          />
        </div>
      </div>
      <ScorePanel
        {view}
        {check}
        rows={rangeRows}
        {range}
        {spots}
        {clusterStatus}
        onspot={(s) => entry?.useSpot(s.call, s.band, s.comment)}
      />
    </main>
  {:else}
    <section class="welcome">
      <h1>OpenVKCL</h1>
      <p>A contest logger for Australian and New Zealand contests.</p>
      <div class="actions">
        <button class="primary" onclick={() => (setupDialog = { initial: newLogSetup(), isNew: true })} disabled={!contests.length}>
          Start a new log
        </button>
        <button onclick={openLog}>Open an existing log</button>
      </div>
      {#if contests.length}
        <p class="muted">Contests: {contests.map((c) => c.name).join(" · ")}</p>
      {/if}
      {#each contestErrors as err}
        <p class="error">{err}</p>
      {/each}
      {#if version}<p class="muted">Version {version}</p>{/if}
    </section>
  {/if}
</div>

{#if setupDialog}
  <SetupDialog
    {contests}
    initial={setupDialog.initial}
    isNew={setupDialog.isNew}
    onsave={saveSetup}
    onclose={() => {
      setupDialog = null;
      refocus();
    }}
  />
{/if}

{#if aboutDialog}
  <AboutDialog
    {version}
    onclose={() => {
      aboutDialog = false;
      refocus();
    }}
  />
{/if}

{#if rigDialog && settings}
  <RigDialog
    initial={settings.rig}
    status={rig}
    onconnect={connectRig}
    onclose={() => {
      rigDialog = false;
      refocus();
    }}
  />
{/if}

{#if lookupDialog && settings}
  <LookupDialog
    initial={settings.qrz}
    initialCluster={settings.cluster}
    stationCall={settings.station.callsign}
    onsave={saveLookups}
    onctychange={async () => {
      const v = await api.getLog();
      if (v) setView(v);
    }}
    onclose={() => {
      lookupDialog = false;
      refocus();
    }}
  />
{/if}

{#if editRow && view}
  <EditQsoDialog
    row={editRow}
    contest={view.contest}
    onsave={saveQso}
    ondelete={deleteQso}
    onclose={() => {
      editRow = null;
      refocus();
    }}
  />
{/if}

{#if adifDialog && view}
  <AdifDialog
    kind={adifDialog.kind}
    file={adifDialog.file}
    {general}
    rows={view.qsos}
    initial={adifDialog.kind === "export" && general ? range : { from: "", to: "" }}
    onimport={importAdif}
    onexport={exportAdif}
    onclose={() => {
      adifDialog = null;
      refocus();
    }}
  />
{/if}

{#if cabrillo != null}
  <CabrilloDialog
    text={cabrillo}
    onsave={saveCabrillo}
    onclose={() => {
      cabrillo = null;
      refocus();
    }}
  />
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: auto auto 1fr;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 14px;
    background: var(--panel-2);
    border-bottom: 1px solid var(--border);
  }
  .title {
    display: flex;
    gap: 12px;
    align-items: baseline;
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .title .call {
    font-weight: 700;
    color: var(--accent);
  }
  .rig {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--muted);
  }
  .rig.on .dot {
    background: var(--ok);
  }
  .theme-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    background: none;
    border-color: transparent;
    color: var(--muted);
  }
  .theme-toggle:hover:not(:disabled) {
    color: var(--fg);
  }
  nav {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 14px;
    border-bottom: 1px solid var(--border);
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 4px;
  }
  .notice {
    margin-left: auto;
    font-size: 0.85rem;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .notice.error {
    color: var(--bad);
  }
  main {
    display: grid;
    /* minmax: a wide log table scrolls instead of pushing the side panel off screen. */
    grid-template-columns: minmax(0, 1fr) 280px;
    gap: 12px;
    padding: 12px 14px;
    min-height: 0;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    min-width: 0;
  }
  .table {
    flex: 1;
    min-height: 0;
  }
  .welcome {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    text-align: center;
    padding: 24px;
  }
  .welcome h1 {
    margin: 0;
    font-size: 2.2rem;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin: 10px 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
