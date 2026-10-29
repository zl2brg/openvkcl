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
    RIG_STATUS_EVENT,
    type CheckResult,
    type ContestDef,
    type LogSetup,
    type LogView,
    type Qso,
    type QsoInput,
    type QsoRow,
    type RigStatus,
    type Settings,
  } from "$lib/api";
  import EntryLine from "$lib/components/EntryLine.svelte";
  import LogTable from "$lib/components/LogTable.svelte";
  import ScorePanel from "$lib/components/ScorePanel.svelte";
  import SetupDialog from "$lib/components/SetupDialog.svelte";
  import RigDialog from "$lib/components/RigDialog.svelte";
  import EditQsoDialog from "$lib/components/EditQsoDialog.svelte";
  import CabrilloDialog from "$lib/components/CabrilloDialog.svelte";

  const LOG_FILTER = [{ name: "OpenVKCL log", extensions: ["ovkcl"] }];

  let contests = $state<ContestDef[]>([]);
  let contestErrors = $state<string[]>([]);
  let settings = $state<Settings | null>(null);
  let view = $state<LogView | null>(null);
  let logVersion = $state(0);
  let rig = $state<RigStatus>({ connected: false, freq_hz: null, band: null, mode: null, rig_mode: null, error: null });
  let now = $state(new Date());
  let version = $state("");
  let notice = $state("");
  let noticeIsError = $state(false);
  let check = $state<CheckResult | null>(null);

  let setupDialog = $state<{ initial: LogSetup; isNew: boolean } | null>(null);
  let rigDialog = $state(false);
  let editRow = $state<QsoRow | null>(null);
  let cabrillo = $state<string | null>(null);

  let entry: EntryLine | undefined = $state();

  let utcClock = $derived(`${pad(now.getUTCHours())}:${pad(now.getUTCMinutes())}:${pad(now.getUTCSeconds())}`);
  let fileName = $derived(view?.path.split(/[\\/]/).pop() ?? "");

  function say(text: string, isError = false) {
    notice = text;
    noticeIsError = isError;
  }

  function setView(v: LogView) {
    view = v;
    logVersion++;
  }

  async function refocus() {
    await tick();
    entry?.focusCall();
  }

  onMount(() => {
    const clock = setInterval(() => (now = new Date()), 1000);
    const unlisten = listen<RigStatus>(RIG_STATUS_EVENT, (e) => (rig = e.payload));
    (async () => {
      try {
        const list = await api.listContests();
        contests = list.contests;
        contestErrors = list.errors;
        settings = await api.getSettings();
        version = await api.appVersion();
        rig = await api.rigStatus();
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
    })();
    return () => {
      clearInterval(clock);
      unlisten.then((f) => f());
    };
  });

  function newLogSetup(): LogSetup {
    const contest = contests[0];
    const today = new Date();
    const date = `${today.getUTCFullYear()}-${pad(today.getUTCMonth() + 1)}-${pad(today.getUTCDate())}`;
    return {
      contest_id: contest?.id ?? "",
      start: new Date(`${date}T${contest?.default_start_utc ?? "00:00"}:00Z`).toISOString(),
      station: settings ? { ...settings.station } : emptyStation(),
      sent_number: "",
      serial_per_band: false,
      utc_offset_minutes: -today.getTimezoneOffset(),
      location: "",
      categories: Object.fromEntries((contest?.categories ?? []).map((c) => [c.key, c.default])),
      soapbox: "",
    };
  }

  function withExt(path: string, ext: string) {
    return path.toLowerCase().endsWith(`.${ext}`) ? path : `${path}.${ext}`;
  }

  async function createLog(setup: LogSetup) {
    const year = new Date(setup.start).getUTCFullYear();
    const path = await save({
      title: "Save new log as",
      defaultPath: `${setup.contest_id}-${year}-${setup.station.callsign.replace(/\//g, "_")}.ovkcl`,
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
    setView(await api.logQso(input));
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
      defaultPath: `${view.setup.station.callsign.replace(/\//g, "_")}-${view.contest.id}.log`,
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

  async function exportAdif() {
    if (!view) return;
    const path = await save({
      title: "Export ADIF",
      defaultPath: `${view.setup.station.callsign.replace(/\//g, "_")}-${view.contest.id}.adi`,
      filters: [{ name: "ADIF", extensions: ["adi"] }],
    });
    if (!path) return;
    try {
      await api.exportAdif(withExt(path, "adi"));
      say(`Exported ADIF to ${withExt(path, "adi")}`);
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
      {#if rig.connected}
        <span class="mono">{formatFreq(rig.freq_hz)} kHz</span>
        <span>{rig.rig_mode ?? ""}</span>
        <span class="muted">{rig.band ?? "out of band"}</span>
      {:else}
        <span class="muted" title={rig.error ?? ""}>{rig.error ? "Radio not responding" : "No radio"}</span>
      {/if}
    </div>
    <div class="clock mono">{utcClock} UTC</div>
  </header>

  <nav>
    <button onclick={() => (setupDialog = { initial: newLogSetup(), isNew: true })} disabled={!contests.length}>New log</button>
    <button onclick={openLog}>Open log</button>
    <button onclick={() => view && (setupDialog = { initial: view.setup, isNew: false })} disabled={!view}>Setup</button>
    <span class="sep"></span>
    <button onclick={showCabrillo} disabled={!view}>Cabrillo…</button>
    <button onclick={exportAdif} disabled={!view}>Export ADIF</button>
    <span class="sep"></span>
    <button onclick={() => (rigDialog = true)}>Radio…</button>
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
          contest={view.contest}
          setup={view.setup}
          {rig}
          {logVersion}
          onlog={logQso}
        />
        <div class="table">
          <LogTable
            rows={view.qsos}
            contest={view.contest}
            highlight={check?.previous[0]?.call ?? ""}
            onedit={(r) => (editRow = r)}
          />
        </div>
      </div>
      <ScorePanel {view} {now} {check} />
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
  .clock {
    font-size: 1.05rem;
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
    grid-template-columns: 1fr 280px;
    gap: 12px;
    padding: 12px 14px;
    min-height: 0;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
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
