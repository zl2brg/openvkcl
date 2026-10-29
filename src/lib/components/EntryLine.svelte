<script lang="ts">
  import { untrack } from "svelte";
  import {
    api,
    errorText,
    hhmm,
    parseUtcText,
    utcText,
    type CheckResult,
    type ContestDef,
    type LogSetup,
    type QsoInput,
    type RigStatus,
  } from "$lib/api";

  let {
    contest,
    setup,
    rig,
    logVersion,
    onlog,
    check = $bindable(null),
  }: {
    contest: ContestDef;
    setup: LogSetup;
    rig: RigStatus;
    /** Changes whenever the log changes, so dupe checks are refreshed. */
    logVersion: number;
    onlog: (input: QsoInput) => Promise<void>;
    check?: CheckResult | null;
  } = $props();

  let call = $state("");
  let sentRst = $state("");
  let rcvdRst = $state("");
  let rcvdNr = $state("");
  let rcvdLoc = $state("");
  let band = $state("");
  let mode = $state("");
  let freqKhz = $state<number | null>(null);
  let followRig = $state(true);
  let manualTime = $state(false);
  let timeText = $state(utcText(new Date().toISOString()));
  let error = $state("");
  let busy = $state(false);

  let callInput: HTMLInputElement | undefined = $state();
  let nrInput: HTMLInputElement | undefined = $state();
  let locInput: HTMLInputElement | undefined = $state();
  let rstInput: HTMLInputElement | undefined = $state();

  let hasNr = $derived(contest.exchange.includes("serial") || contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let defaultRst = $derived(mode === "PH" ? "59" : "599");
  let rigDriven = $derived(followRig && rig.connected);

  $effect(() => {
    if (!contest.bands.includes(band)) band = contest.bands[0];
    if (!contest.modes.includes(mode)) mode = contest.modes[0];
  });

  $effect(() => {
    if (!rigDriven) return;
    if (rig.band && contest.bands.includes(rig.band)) band = rig.band;
    if (rig.mode && contest.modes.includes(rig.mode)) mode = rig.mode;
    freqKhz = rig.freq_hz != null ? rig.freq_hz / 1000 : null;
  });

  /** `useKnown` falls back to the locator last received from this station. */
  function input(useKnown: boolean): QsoInput {
    return {
      time: manualTime ? parseUtcText(timeText) : null,
      call: call.trim().toUpperCase(),
      band,
      mode,
      freq_khz: rigDriven ? freqKhz : null,
      sent_rst: sentRst.trim(),
      rcvd_rst: rcvdRst.trim(),
      rcvd_nr: rcvdNr.trim(),
      rcvd_locator: rcvdLoc.trim() || (useKnown ? check?.known_locator : "") || "",
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

  export function focusCall() {
    callInput?.focus();
  }

  function clear() {
    call = "";
    rcvdRst = "";
    rcvdNr = "";
    rcvdLoc = "";
    error = "";
    untrack(() => callInput?.focus());
  }

  async function submit() {
    if (busy || !call.trim()) return;
    if (manualTime && !parseUtcText(timeText)) {
      error = "Time must be YYYY-MM-DD HH:MM (UTC).";
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
    return [callInput, hasNr ? nrInput : undefined, hasLoc ? locInput : undefined, rstInput].filter(
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
    } else if (e.key === " " && fieldOrder().includes(e.target as HTMLInputElement)) {
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

  let sentText = $derived(
    [sentRst || defaultRst, hasNr ? check?.sent_nr : "", hasLoc ? setup.station.locator.toUpperCase() : ""]
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
    <label class="check"><input type="checkbox" bind:checked={manualTime} /> Enter time</label>
    {#if manualTime}
      <input class="mono time" bind:value={timeText} title="QSO time, UTC (YYYY-MM-DD HH:MM)" />
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
    <label class="sent">
      Sent
      <input class="mono" value={sentText} readonly tabindex="-1" />
    </label>
    {#if hasNr}
      <label class="nr">
        {contest.number_label}
        <input class="mono" bind:this={nrInput} bind:value={rcvdNr} inputmode="numeric" autocomplete="off" />
      </label>
    {/if}
    {#if hasLoc}
      <label class="loc">
        Locator
        <input
          class="mono"
          bind:this={locInput}
          bind:value={rcvdLoc}
          maxlength="6"
          placeholder={check?.known_locator ?? ""}
          autocomplete="off"
          spellcheck="false"
        />
      </label>
    {/if}
    <label class="rst">
      RST rcvd
      <input class="mono" bind:this={rstInput} bind:value={rcvdRst} placeholder={defaultRst} autocomplete="off" />
    </label>
    <label class="rst">
      RST sent
      <input class="mono" bind:value={sentRst} placeholder={defaultRst} autocomplete="off" />
    </label>
    <button class="primary log" onclick={submit} disabled={busy || !call.trim()}>Log ⏎</button>
  </div>

  <div class="status">
    {#if error}
      <span class="bad">{error}</span>
    {:else if call.trim() && check}
      {@const r = check.result}
      {#if r.invalid}
        <span class="bad">✗ {r.invalid}</span>
      {:else if r.dupe}
        <span class="bad strong">DUPE</span>
        {#if r.next_ok}
          <span>can work again from {hhmm(r.next_ok)} UTC</span>
        {:else}
          <span>already worked on this {contest.rework.per.includes("band") ? "band and " : ""}mode</span>
        {/if}
      {:else}
        <span class="ok strong">OK</span>
        <span>{r.points.toFixed(r.points % 1 ? 1 : 0)} pts</span>
        {#if r.distance_km != null}<span>{r.distance_km.toFixed(0)} km</span>{/if}
        {#if r.new_mult}<span class="mult">NEW MULT {r.prefix}</span>{/if}
        {#if r.warning}<span class="warn">{r.warning}</span>{/if}
      {/if}
    {:else}
      <span class="muted">Space moves between fields · Enter logs · Esc clears · Alt+arrows change band/mode</span>
    {/if}
  </div>
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
    gap: 8px;
    align-items: end;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.75rem;
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
    width: 11ch;
  }
  .sent input {
    width: 13ch;
    color: var(--muted);
  }
  .nr input {
    width: 6ch;
  }
  .loc input {
    width: 7ch;
  }
  .rst input {
    width: 4.5ch;
  }
  .time {
    width: 17ch;
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
</style>
