<script lang="ts">
  import { untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import BandGrid from "./BandGrid.svelte";
  import KeyerBar from "./KeyerBar.svelte";
  import {
    api,
    defaultRst as rstFor,
    errorText,
    dateTimeText,
    generalMode,
    isFutureQsoTime,
    modeClass,
    parseDateTimeText,
    zoneLabel,
    type CheckResult,
    type ContestDef,
    type KeyerConfig,
    type Need,
    type LogSetup,
    type QrzInfo,
    type QsoInput,
    type RigStatus,
    type ScpMatches,
  } from "$lib/api";
  import Countdown from "./Countdown.svelte";

  let {
    contest,
    setup,
    rig,
    logVersion,
    worked,
    qrzEnabled,
    keyer,
    onkeyerchange,
    onlog,
    check = $bindable(null),
    typed = $bindable(""),
  }: {
    contest: ContestDef;
    setup: LogSetup;
    rig: RigStatus;
    /** Changes whenever the log changes, so dupe checks are refreshed. */
    logVersion: number;
    /** Calls already in the log, highlighted in the Super Check Partial list. */
    worked: Set<string>;
    qrzEnabled: boolean;
    /** Voice keyer settings, for the F-key bar; none until settings load. */
    keyer: KeyerConfig | null;
    onkeyerchange: (config: KeyerConfig) => void;
    onlog: (input: QsoInput) => Promise<void>;
    check?: CheckResult | null;
    /** The call being typed, in capitals, for the log to filter on. */
    typed?: string;
  } = $props();

  let call = $state("");
  let sentRst = $state("");
  let rcvdRst = $state("");
  let rcvdNr = $state("");
  let rcvdExch = $state("");
  let rcvdLoc = $state("");
  let name = $state("");
  let comment = $state("");
  let band = $state("");
  let mode = $state("");
  let freqKhz = $state<number | null>(null);
  let followRig = $state(true);
  let manualTime = $state(false);
  let timeText = $state("");
  let futureTime = $derived(manualTime && isFutureQsoTime(parseDateTimeText(timeText)));
  let error = $state("");
  let busy = $state(false);

  let callInput: HTMLInputElement | undefined = $state();
  let nrInput: HTMLInputElement | undefined = $state();
  let exchInput: HTMLInputElement | undefined = $state();
  let locInput: HTMLInputElement | undefined = $state();
  let rstInput: HTMLInputElement | undefined = $state();
  let rstSentInput: HTMLInputElement | undefined = $state();
  let nameInput: HTMLInputElement | undefined = $state();
  let commentInput: HTMLInputElement | undefined = $state();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let general = $derived(contest.general);
  let defaultRst = $derived(rstFor(mode));
  // Band and mode follow the radio only while it reports a frequency.
  let rigDriven = $derived(followRig && rig.connected && rig.freq_hz != null);

  $effect(() => {
    // General logs list every band from 2190m up; start on 20m rather than the first.
    if (!contest.bands.includes(band)) band = contest.general && contest.bands.includes("20m") ? "20m" : contest.bands[0];
    if (!contest.modes.includes(mode)) mode = contest.modes[0];
  });

  $effect(() => {
    if (!rigDriven) return;
    if (rig.band && contest.bands.includes(rig.band)) band = rig.band;
    if (general) {
      const m = rig.rig_mode ? generalMode(rig.rig_mode, untrack(() => mode)) : null;
      if (m && contest.modes.includes(m)) mode = m;
    } else if (rig.mode && contest.modes.includes(rig.mode)) mode = rig.mode;
    freqKhz = rig.freq_hz != null ? rig.freq_hz / 1000 : null;
  });

  /** `useKnown` falls back to the locator and number last received from this station. */
  function input(useKnown: boolean): QsoInput {
    return {
      time: manualTime ? parseDateTimeText(timeText) : null,
      call: call.trim().toUpperCase(),
      band,
      mode,
      freq_khz: rigDriven ? freqKhz : null,
      sent_rst: sentRst.trim(),
      rcvd_rst: rcvdRst.trim(),
      rcvd_nr: rcvdNr.trim(),
      rcvd_exch: rcvdExch.trim() || (useKnown ? check?.known_exch : "") || "",
      rcvd_locator: rcvdLoc.trim() || (useKnown ? check?.known_locator : "") || "",
      name: name.trim() || (useKnown ? knownName : "") || "",
      qth: "",
      comment: comment.trim(),
    };
  }

  $effect(() => {
    const draft = input(false);
    logVersion;
    const timer = setTimeout(async () => {
      try {
        check = await api.checkQso(draft);
      } catch {
        check = null;
      }
    }, 80);
    return () => clearTimeout(timer);
  });

  let scp = $state<ScpMatches | null>(null);
  let qrz = $state<QrzInfo | null>(null);
  let qrzError = $state("");
  /** The last contact with this station that has a name or QTH, from the log. */
  let fromLog = $derived.by(() => {
    const previous = check?.previous ?? [];
    for (let i = previous.length - 1; i >= 0; i--) {
      if (previous[i].name || previous[i].qth) return previous[i];
    }
    return null;
  });

  let callKey = $derived(call.trim().toUpperCase());
  $effect(() => {
    typed = callKey;
  });
  /** The name the log or QRZ.com already has for this station, used when left blank. */
  /** Show the voice keyer's F-key bar: kept on this computer between runs. */
  let showKeyer = $state(localStorage.getItem("showKeyer") !== "0");
  $effect(() => localStorage.setItem("showKeyer", showKeyer ? "1" : "0"));

  let knownName = $derived(check?.known_name ?? qrz?.name ?? "");
  let plausible = $derived(/^[A-Z0-9/]{3,}$/.test(callKey) && /\d/.test(callKey) && /[A-Z]/.test(callKey));

  $effect(() => {
    const partial = callKey;
    logVersion;
    const timer = setTimeout(async () => {
      try {
        const m = await api.scpLookup(partial);
        if (partial === untrack(() => callKey)) scp = m;
      } catch {
        scp = null;
      }
    }, 60);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    const target = callKey;
    qrz = null;
    qrzError = "";
    if (!qrzEnabled || !plausible) return;
    const timer = setTimeout(async () => {
      try {
        const info = await api.qrzLookup(target);
        if (target !== untrack(() => callKey)) return;
        qrz = info;
        if (!info) qrzError = "not on QRZ.com";
      } catch (e) {
        if (target === untrack(() => callKey)) qrzError = errorText(e);
      }
      // Soon after typing stops: QRZ.com itself takes most of a second to answer.
    }, 200);
    return () => clearTimeout(timer);
  });

  function pick(c: string) {
    call = c;
    callInput?.focus();
  }

  function useGrid(grid: string) {
    rcvdLoc = grid.toUpperCase().slice(0, 6);
    locInput?.focus();
  }

  function openQrz(e: MouseEvent) {
    e.preventDefault();
    openUrl(`https://www.qrz.com/db/${encodeURIComponent(callKey)}`);
  }

  export function focusCall() {
    callInput?.focus();
  }

  /**
   * Work a DX cluster spot: fill in the call and, unless the radio sets them,
   * the spot's band and any mode its comment names (e.g. "FT8 -12dB").
   */
  export function useSpot(spotCall: string, spotBand: string | null, comment = "") {
    call = spotCall.toUpperCase();
    if (!rigDriven) {
      if (spotBand && contest.bands.includes(spotBand)) band = spotBand;
      const named = comment
        .toUpperCase()
        .split(/[^A-Z0-9]+/)
        .map((w) => (w === "USB" || w === "LSB" ? "SSB" : w))
        .find((w) => contest.general && contest.modes.includes(w));
      if (named) mode = named;
    }
    callInput?.focus();
  }

  function clear() {
    call = "";
    scp = null;
    rcvdRst = "";
    sentRst = "";
    rcvdNr = "";
    rcvdExch = "";
    rcvdLoc = "";
    name = "";
    comment = "";
    error = "";
    untrack(() => callInput?.focus());
  }

  async function submit() {
    if (busy || !call.trim()) return;
    if (manualTime && !parseDateTimeText(timeText)) {
      error = `Time must be YYYY-MM-DD HH:MM (${zoneLabel()}).`;
      return;
    }
    busy = true;
    try {
      await onlog(input(true));
      clear();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function fieldOrder(): HTMLInputElement[] {
    if (general) {
      return [callInput, rstSentInput, rstInput, nameInput, commentInput].filter(
        (x): x is HTMLInputElement => !!x,
      );
    }
    return [
      callInput,
      rstInput,
      hasNr ? nrInput : undefined,
      hasExch ? exchInput : undefined,
      hasLoc ? locInput : undefined,
    ].filter(
      (x): x is HTMLInputElement => !!x,
    );
  }

  function step<T>(list: T[], cur: T, dir: number): T {
    const i = list.indexOf(cur);
    return list[(i + dir + list.length) % list.length];
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      submit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      clear();
    } else if (
      e.key === " " &&
      fieldOrder().includes(e.target as HTMLInputElement) &&
      // Names, places and comments have spaces in them.
      !(e.target as HTMLInputElement).dataset.text
    ) {
      e.preventDefault();
      const next = step(fieldOrder(), e.target as HTMLInputElement, 1);
      next.focus();
      next.select();
    } else if (e.altKey && !rigDriven && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      band = step(contest.bands, band, e.key === "ArrowUp" ? 1 : -1);
    } else if (e.altKey && !rigDriven && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      mode = step(contest.modes, mode, e.key === "ArrowRight" ? 1 : -1);
    }
  }

  const CLASS_NAME = { PH: "phone", CW: "CW", DG: "data" };

  function needClass(n: Need) {
    return n === "confirmed" ? "ok" : "warn";
  }

  /** E.g. "Confirmed · 20m worked, not confirmed". */
  function needText(dx: NonNullable<CheckResult["dx"]>): string {
    const word = (n: Need) => (n === "confirmed" ? "confirmed" : "worked, not confirmed");
    const parts = [`Country ${word(dx.any)}`];
    if (dx.band !== "new" && dx.band !== dx.any) parts.push(`${band} ${word(dx.band)}`);
    if (dx.mode !== "new" && dx.mode !== dx.any) parts.push(`${CLASS_NAME[modeClass(mode)]} ${word(dx.mode)}`);
    return parts.join(" · ");
  }

  let sentText = $derived(
    [
      sentRst || defaultRst,
      hasNr ? check?.sent_nr : "",
      hasExch ? setup.sent_number : "",
      hasLoc ? setup.station.locator.toUpperCase() : "",
    ]
      .filter(Boolean)
      .join(" "),
  );
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="entry" role="group" aria-label="QSO entry" {onkeydown}>
  <div class="controls">
    <label>
      Band
      <select bind:value={band} disabled={rigDriven}>
        {#each contest.bands as b (b)}<option value={b}>{b}</option>{/each}
      </select>
    </label>
    <label>
      Mode
      <select bind:value={mode} disabled={rigDriven}>
        {#each contest.modes as m (m)}<option value={m}>{m}</option>{/each}
      </select>
    </label>
    {#if rig.connected}
      <label class="check"><input type="checkbox" bind:checked={followRig} /> Follow radio</label>
    {/if}
    <label class="check">
      <input
        type="checkbox"
        bind:checked={manualTime}
        onchange={() => manualTime && (timeText = dateTimeText(new Date().toISOString()))}
      />
      Enter time
    </label>
    {#if keyer}
      <label class="check" title="Show the F-key voice messages. Hidden, the F-keys don't play anything.">
        <input type="checkbox" bind:checked={showKeyer} /> Voice keys
      </label>
    {/if}
    {#if manualTime}
      <input class="mono time" bind:value={timeText} title="QSO time, {zoneLabel()} (YYYY-MM-DD HH:MM)" />
      {#if futureTime}
        <span class="warn">QSO time is in the future</span>
      {/if}
    {/if}
    {#if general}
      <div class="bandgrid">
        <BandGrid
          bands={contest.bands}
          {band}
          {mode}
          call={callKey}
          previous={callKey ? (check?.previous ?? []) : []}
          dx={callKey ? (check?.dx ?? null) : null}
        />
      </div>
    {/if}
  </div>

  <div class="fields">
    <label class="call">
      Call
      <input
        class="mono"
        bind:this={callInput}
        bind:value={call}
        oninput={() => (call = call.toUpperCase())}
        autocomplete="off"
        spellcheck="false"
      />
    </label>
    <label class="rst">
      RST sent
      <input class="mono" bind:this={rstSentInput} bind:value={sentRst} placeholder={defaultRst} autocomplete="off" />
    </label>
    <label class="rst">
      RST rcvd
      <input class="mono" bind:this={rstInput} bind:value={rcvdRst} placeholder={defaultRst} autocomplete="off" />
    </label>
    {#if !general}
      <label class="sent">
        Sent
        <input
          class="mono"
          value={sentText}
          readonly
          tabindex="-1"
          style:width="{Math.max(8, sentText.length + 2)}ch"
        />
      </label>
    {/if}
    {#if hasNr}
      <label class="nr">
        Nr rcvd
        <input class="mono" bind:this={nrInput} bind:value={rcvdNr} inputmode="numeric" autocomplete="off" />
      </label>
    {/if}
    {#if hasExch}
      <label class="nr">
        {contest.number_label}
        <input
          class="mono"
          bind:this={exchInput}
          bind:value={rcvdExch}
          placeholder={check?.known_exch ?? ""}
          inputmode="numeric"
          autocomplete="off"
        />
      </label>
    {/if}
    {#if hasLoc}
      <label class="loc">
        Locator
        <input
          class="mono"
          bind:this={locInput}
          bind:value={rcvdLoc}
          maxlength="10"
          placeholder={check?.known_locator ?? ""}
          autocomplete="off"
          spellcheck="false"
        />
      </label>
    {/if}
    {#if general}
      <label class="text">
        Name
        <input bind:this={nameInput} bind:value={name} placeholder={knownName} data-text="1" autocomplete="off" />
      </label>
      <label class="text comment">
        Comment
        <input bind:this={commentInput} bind:value={comment} data-text="1" autocomplete="off" />
      </label>
    {/if}
    <button class="primary log" onclick={submit} disabled={busy || !call.trim()}>Log ⏎</button>
  </div>

  <div class="status">
    {#if error}
      <span class="bad">{error}</span>
    {:else if general && call.trim() && check}
      {@const dx = check.dx}
      {#if dx}
        {@const c = dx.country}
        <span class="country">{c.country}</span>
        <span class="muted">{[c.cont, c.cqz ? `CQ ${c.cqz}` : "", c.ituz ? `ITU ${c.ituz}` : ""].filter(Boolean).join(" · ")}</span>
        {#if dx.any === "new"}
          <span class="mult strong">NEW COUNTRY</span>
        {:else}
          {#if dx.band === "new"}<span class="mult strong">NEW BAND</span>{/if}
          {#if dx.mode === "new"}<span class="mult strong">NEW MODE</span>{/if}
          <span class={needClass(dx.any)}>{needText(dx)}</span>
        {/if}
      {:else if plausible}
        <span class="muted">Country not known: download the country file in Lookups…</span>
      {/if}
      {#if check.previous.length}
        <span class="muted">· worked {check.previous.length}× before</span>
      {/if}
    {:else if call.trim() && check}
      {@const r = check.result}
      {#if r.invalid}
        <span class="bad">✗ {r.invalid}</span>
      {:else if r.dupe}
        <span class="bad strong">DUPE</span>
        {#if r.next_ok}
          <Countdown at={r.next_ok} mode="until" />
        {:else}
          <span>already worked on this {contest.rework.per.includes("band") ? "band and " : ""}mode</span>
        {/if}
      {:else}
        <span class="ok strong">OK</span>
        <span>{r.points.toFixed(r.points % 1 ? 1 : 0)} pts</span>
        {#if r.distance_km != null}<span>{r.distance_km.toFixed(0)} km</span>{/if}
        {#if r.new_mult}<span class="mult">NEW MULT {r.mult}</span>{/if}
        {#if r.warning}<span class="warn">{r.warning}</span>{/if}
      {/if}
    {:else}
      <span class="muted"
        >Space moves between {general ? "call and report " : ""}fields · Enter logs · Esc clears · Alt+arrows change
        band/mode</span
      >
    {/if}
  </div>

  <!-- Empty rows hold the suggestions' and QRZ line's places when they're not
       shown, so the panel doesn't change height as a call is typed and the log
       below stays put. -->
  {#if !(callKey && scp && (scp.partial.length || scp.near.length))}
    <div class="scp"></div>
  {/if}
  {#if !plausible}
    <div class="qrz"></div>
  {/if}

  {#if callKey && scp && (scp.partial.length || scp.near.length)}
    <div class="scp mono" aria-label="Super Check Partial">
      {#each scp.partial as c (c)}
        <button tabindex="-1" class:worked={worked.has(c)} onclick={() => pick(c)} title={worked.has(c) ? "In log" : ""}>{c}</button>
      {/each}
      {#if scp.near.length}
        <span class="label" title="Calls one character different from what you typed">N+1</span>
        {#each scp.near as c (c)}
          <button tabindex="-1" class="near" class:worked={worked.has(c)} onclick={() => pick(c)}>{c}</button>
        {/each}
      {/if}
    </div>
  {/if}

  {#if plausible}
    <div class="qrz">
      {#if scp?.exact}<span class="ok" title="In MASTER.SCP or your log">✓ known call</span>{/if}
      {#if qrz}
        {@const place = [qrz.city, qrz.state, qrz.country].filter(Boolean).join(", ")}
        {#if qrz.name}<strong>{qrz.name}</strong>{/if}
        {#if place}<span>{place}</span>{/if}
        {#if qrz.grid}
          {#if hasLoc && !rcvdLoc.trim()}
            <button tabindex="-1" class="grid mono" onclick={() => useGrid(qrz!.grid)} title="Use this locator (QRZ.com home locator: check with the station)">
              {qrz.grid}
            </button>
          {:else}
            <span class="mono">{qrz.grid}</span>
          {/if}
        {/if}
      {:else if qrzError}
        <span class="muted">QRZ: {qrzError}</span>
      {:else if fromLog}
        <!-- Shown at once, until QRZ.com answers. -->
        {#if fromLog.name}<strong>{fromLog.name}</strong>{/if}
        {#if fromLog.qth}<span>{fromLog.qth}</span>{/if}
        <span class="muted">from your log</span>
      {/if}
      <a class="link" href="https://www.qrz.com/db/{callKey}" onclick={openQrz} tabindex="-1">QRZ.com ↗</a>
    </div>
  {/if}

  {#if keyer && showKeyer}
    <KeyerBar
      config={keyer}
      phone={modeClass(mode) === "PH"}
      call={callKey}
      freqHz={rig.connected ? rig.freq_hz : null}
      onchange={onkeyerchange}
    />
  {/if}
</div>

<style>
  .entry {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .controls {
    display: flex;
    gap: 14px;
    align-items: end;
    flex-wrap: wrap;
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: end;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--muted);
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--fg);
  }
  .fields input {
    font-size: 1.35rem;
    padding: 6px 8px;
  }
  .call input {
    width: 13ch;
  }
  .sent input {
    color: var(--muted);
  }
  .nr input {
    width: 6ch;
  }
  .loc input {
    width: 11ch;
  }
  .rst input {
    width: 5.5ch;
  }
  .text input {
    width: 16ch;
  }
  .comment {
    flex: 1;
    min-width: 10ch;
  }
  .comment input {
    width: 100%;
  }
  .country {
    font-weight: 700;
  }
  .bandgrid {
    margin-left: auto;
  }
  .time {
    width: 19ch;
  }
  .log {
    height: 2.6rem;
    margin-left: auto;
  }
  .status {
    height: 1.4rem;
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 0.95rem;
    white-space: nowrap;
    overflow: hidden;
  }
  .strong {
    font-weight: 700;
    letter-spacing: 0.05em;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
  .warn {
    color: var(--warn);
  }
  .mult {
    color: var(--accent);
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .scp {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
    align-content: flex-start;
    /* Two lines of suggestions, whether or not there are any. */
    height: 3.6rem;
    overflow: hidden;
  }
  .scp button,
  .qrz .grid {
    padding: 1px 6px;
    font-size: 0.9rem;
  }
  .scp button.near {
    color: var(--muted);
  }
  .scp button.worked {
    color: var(--accent);
    border-color: var(--accent);
  }
  .scp .label {
    font-size: 0.75rem;
    color: var(--muted);
    margin: 0 2px 0 8px;
  }
  .qrz {
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 0.9rem;
    height: 1.6rem;
    white-space: nowrap;
    overflow: hidden;
  }
  .qrz .link {
    margin-left: auto;
    color: var(--accent);
    font-size: 0.85rem;
  }
</style>
