<script lang="ts">
  import { untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import BandGrid from "./BandGrid.svelte";
  import KeyerBar from "./KeyerBar.svelte";
  import CodeSuggest from "./CodeSuggest.svelte";
  import {
    api,
    defaultRst as rstFor,
    reportProblem,
    errorText,
    dateTimeText,
    generalMode,
    isFutureQsoTime,
    isRover,
    modeClass,
    numberMeaning,
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
    onmove,
    check = $bindable(null),
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
    /** A rover has moved: choose the code it sends from now on. */
    onmove: () => void;
    check?: CheckResult | null;
  } = $props();

  let call = $state("");
  let sentRst = $state("");
  let rcvdRst = $state("");
  let rcvdNr = $state("");
  let rcvdExch = $state("");
  let sentExch = $state("");
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
  let sentExchInput: HTMLInputElement | undefined = $state();
  let locInput: HTMLInputElement | undefined = $state();
  let rstInput: HTMLInputElement | undefined = $state();
  let rstSentInput: HTMLInputElement | undefined = $state();
  let nameInput: HTMLInputElement | undefined = $state();
  let commentInput: HTMLInputElement | undefined = $state();
  let logButton: HTMLButtonElement | undefined = $state();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasText = $derived(contest.exchange.includes("text"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let hasRst = $derived(!contest.generic || contest.exchange.includes("rst"));
  let general = $derived(contest.general);
  let defaultRst = $derived(rstFor(mode));
  let sentProblem = $derived(hasRst ? reportProblem(mode, sentRst) : null);
  let rcvdProblem = $derived(hasRst ? reportProblem(mode, rcvdRst) : null);
  /** What's wrong with the reports, shown in the status line until fixed. */
  let rstProblem = $derived(
    sentProblem ? `RST sent ${sentProblem}` : rcvdProblem ? `RST rcvd ${rcvdProblem}` : "",
  );
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
      sent_exch: sentExch.trim(),
      rcvd_locator: rcvdLoc.trim() || (useKnown ? check?.known_locator : "") || "",
      name: name.trim() || (useKnown ? knownName : "") || "",
      qth: "",
      comment: comment.trim(),
    };
  }

  let checkSeq = 0;
  $effect(() => {
    const draft = input(false);
    logVersion;
    const seq = ++checkSeq;
    const timer = setTimeout(async () => {
      try {
        const result = await api.checkQso(draft);
        if (seq === checkSeq) check = result;
      } catch {
        if (seq === checkSeq) check = null;
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

  /** The number field takes codes from an official list (e.g. VK Shires "BU4"), not just digits. */
  let coded = $derived(!!contest.number_list);
  let exchSuggest = $state<CodeSuggest>();
  let exchFocused = $state(false);
  /** What the received code stands for, e.g. "Bundaberg, Qld" or "CQ zone 32". */
  let exchMeaning = $derived.by(() => {
    const code = rcvdExch.trim() || check?.known_exch || "";
    return hasExch && code ? numberMeaning(contest, code) : null;
  });

  let callKey = $derived(call.trim().toUpperCase());
  /** The name the log or QRZ.com already has for this station, used when left blank. */
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
    sentExch = "";
    rcvdLoc = "";
    name = "";
    comment = "";
    error = "";
    untrack(() => callInput?.focus());
  }

  async function submit() {
    if (busy || !call.trim()) return;
    if (rstProblem) {
      (sentProblem ? rstSentInput : rstInput)?.focus();
      return;
    }
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
      hasRst ? rstSentInput : undefined,
      hasRst ? rstInput : undefined,
      hasNr ? nrInput : undefined,
      hasText ? sentExchInput : undefined,
      hasExch || hasText ? exchInput : undefined,
      hasLoc ? locInput : undefined,
    ].filter((x): x is HTMLInputElement => !!x);
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
    } else if (e.key === "Tab" && !e.altKey && !e.ctrlKey && !e.metaKey) {
      // Tab / Shift+Tab cycle the same fields as Space, including the text ones,
      // plus the Log button (skipped while it is disabled, as it can't take focus).
      const stops: HTMLElement[] = [...fieldOrder()];
      if (logButton && !logButton.disabled) stops.push(logButton);
      if (stops.includes(e.target as HTMLElement)) {
        e.preventDefault();
        const next = step(stops, e.target as HTMLElement, e.shiftKey ? -1 : 1);
        next.focus();
        if (next instanceof HTMLInputElement) next.select();
      }
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
      hasText ? sentExch : "",
      hasLoc ? setup.station.locator.toUpperCase() : "",
    ]
      .filter(Boolean)
      .join(" "),
  );
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="entry" class:steady={!!keyer} role="group" aria-label="QSO entry" {onkeydown}>
  <div class="controls">
    <label>
      Band
      <select
        value={band}
        class:locked={rigDriven}
        tabindex={rigDriven ? -1 : 0}
        title={rigDriven ? "Following the radio" : ""}
        onchange={(e) => {
          if (rigDriven) e.currentTarget.value = band;
          else band = e.currentTarget.value;
        }}
      >
        {#each contest.bands as b (b)}<option value={b}>{b}</option>{/each}
      </select>
    </label>
    <label>
      Mode
      <select
        value={mode}
        class:locked={rigDriven}
        tabindex={rigDriven ? -1 : 0}
        title={rigDriven ? "Following the radio" : ""}
        onchange={(e) => {
          if (rigDriven) e.currentTarget.value = mode;
          else mode = e.currentTarget.value;
        }}
      >
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
    {#if hasRst}
      <label class="rst">
        RST sent
        <input
          class="mono"
          class:invalid={!!sentProblem}
          bind:this={rstSentInput}
          bind:value={sentRst}
          placeholder={defaultRst}
          maxlength="4"
          autocomplete="off"
          title={sentProblem ? `RST sent ${sentProblem}` : ""}
        />
      </label>
      <label class="rst">
        RST rcvd
        <input
          class="mono"
          class:invalid={!!rcvdProblem}
          bind:this={rstInput}
          bind:value={rcvdRst}
          placeholder={defaultRst}
          maxlength="4"
          autocomplete="off"
          title={rcvdProblem ? `RST rcvd ${rcvdProblem}` : ""}
        />
      </label>
    {/if}
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
      {#if isRover(contest, setup)}
        <button
          class="move"
          tabindex="-1"
          onclick={onmove}
          title="You've moved: choose the {contest.number_list?.item ?? contest.number_label} you send from now on"
        >
          Moved…
        </button>
      {/if}
    {/if}
    {#if hasNr}
      <label class="nr">
        Nr rcvd
        <input class="mono" bind:this={nrInput} bind:value={rcvdNr} inputmode="numeric" autocomplete="off" />
      </label>
    {/if}
    {#if hasText}
      <label class="text">
        {contest.number_label} sent
        <input class="mono" bind:this={sentExchInput} bind:value={sentExch} data-text="1" autocomplete="off" />
      </label>
      <label class="text">
        {contest.number_label}
        <input class="mono" bind:this={exchInput} bind:value={rcvdExch} data-text="1" autocomplete="off" />
      </label>
    {/if}
    {#if hasExch}
      <label class="nr" class:coded>
        {contest.number_label}
        <input
          class="mono"
          bind:this={exchInput}
          bind:value={rcvdExch}
          oninput={() => coded && (rcvdExch = rcvdExch.toUpperCase())}
          onkeydown={(e) => exchSuggest?.key(e)}
          onfocus={() => (exchFocused = true)}
          onblur={() => (exchFocused = false)}
          placeholder={check?.known_exch ?? ""}
          inputmode={coded ? "text" : "numeric"}
          autocomplete="off"
          spellcheck="false"
        />
        {#if coded}
          <CodeSuggest
            bind:this={exchSuggest}
            {contest}
            typed={rcvdExch}
            open={exchFocused}
            onpick={(code) => (rcvdExch = code)}
          />
        {/if}
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
    <button class="primary log" bind:this={logButton} onclick={submit} disabled={busy || !call.trim()}>Log ⏎</button>
  </div>

  <div class="status">
    {#if error}
      <span class="bad">{error}</span>
    {:else if rstProblem}
      <span class="bad">{rstProblem}</span>
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
        {#if exchMeaning}<span class="country">{exchMeaning}</span>{/if}
        {#if r.warning}<span class="warn">{r.warning}</span>{/if}
      {/if}
    {:else if contest.number_list_error}
      <span class="warn" title={contest.number_list_error}
        >{contest.number_label} codes can't be checked: {contest.number_list_error}</span
      >
    {:else}
      <span class="muted"
        >{general
          ? "Space moves between call and report fields · Tab also reaches name, comment and the Log Button"
          : "Space or Tab moves between fields · Tab also reaches the Log Button"} · Enter logs · Esc clears · Alt+arrows change
        band/mode</span
      >
    {/if}
  </div>

  {#if keyer}
    <KeyerBar
      config={keyer}
      phone={modeClass(mode) === "PH"}
      call={callKey}
      freqHz={rig.connected ? rig.freq_hz : null}
      onchange={onkeyerchange}
    />
  {/if}

  {#if callKey && scp && !scp.exact && (scp.partial.length || scp.near.length)}
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
  .controls select.locked {
    pointer-events: none;
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
  label.coded {
    position: relative;
  }
  .nr input {
    width: 6ch;
  }
  .loc input {
    width: 11ch;
  }
  .fields input:focus {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .fields input.invalid {
    border-color: var(--bad);
  }
  .fields input.invalid:focus {
    outline: 2px solid var(--bad);
    outline-offset: -1px;
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
    min-height: 1.4rem;
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 0.95rem;
  }
  .entry.steady .status {
    height: 1.4rem;
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
    max-height: 4.2rem;
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
  }
  .qrz .link {
    margin-left: auto;
    color: var(--accent);
    font-size: 0.85rem;
  }
</style>
