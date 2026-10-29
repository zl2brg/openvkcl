<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { parseUtcText, utcText, type ContestDef, type Qso, type QsoRow } from "$lib/api";

  let {
    row,
    contest,
    onsave,
    ondelete,
    onclose,
  }: {
    row: QsoRow;
    contest: ContestDef;
    onsave: (qso: Qso) => void;
    ondelete: (id: number) => void;
    onclose: () => void;
  } = $props();

  const r = untrack(() => $state.snapshot(row)) as QsoRow;
  let qso = $state<Qso>({
    id: r.id,
    time: r.time,
    call: r.call,
    band: r.band,
    mode: r.mode,
    freq_khz: r.freq_khz,
    sent_rst: r.sent_rst,
    sent_nr: r.sent_nr,
    sent_locator: r.sent_locator,
    rcvd_rst: r.rcvd_rst,
    rcvd_nr: r.rcvd_nr,
    rcvd_locator: r.rcvd_locator,
    operator: r.operator,
    sent_exch: r.sent_exch,
    rcvd_exch: r.rcvd_exch,
  });
  let timeText = $state(utcText(r.time));
  let error = $state("");
  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));

  function save() {
    const t = parseUtcText(timeText);
    if (!t) {
      error = "Time must be YYYY-MM-DD HH:MM (UTC).";
      return;
    }
    qso.time = t;
    onsave($state.snapshot(qso) as Qso);
  }
</script>

<Modal title={`Edit QSO with ${row.call}`} {onclose}>
  <div class="form">
    <div class="row">
      <label>Time (UTC) <input class="mono" bind:value={timeText} /></label>
      <label>Call <input class="mono" bind:value={qso.call} /></label>
    </div>
    <div class="row three">
      <label>
        Band
        <select bind:value={qso.band}>
          {#each contest.bands as b (b)}<option value={b}>{b}</option>{/each}
        </select>
      </label>
      <label>
        Mode
        <select bind:value={qso.mode}>
          {#each contest.modes as m (m)}<option value={m}>{m}</option>{/each}
        </select>
      </label>
      <label>Freq (kHz) <input class="mono" type="number" bind:value={qso.freq_khz} /></label>
    </div>
    <h3>Sent</h3>
    <div class="row three">
      <label>RST <input class="mono" bind:value={qso.sent_rst} /></label>
      {#if hasNr}<label>Nr <input class="mono" bind:value={qso.sent_nr} /></label>{/if}
      {#if hasExch}<label>{contest.number_label} <input class="mono" bind:value={qso.sent_exch} /></label>{/if}
      {#if hasLoc}<label>Locator <input class="mono" bind:value={qso.sent_locator} maxlength="10" /></label>{/if}
    </div>
    <h3>Received</h3>
    <div class="row three">
      <label>RST <input class="mono" bind:value={qso.rcvd_rst} /></label>
      {#if hasNr}<label>Nr <input class="mono" bind:value={qso.rcvd_nr} /></label>{/if}
      {#if hasExch}<label>{contest.number_label} <input class="mono" bind:value={qso.rcvd_exch} /></label>{/if}
      {#if hasLoc}<label>Locator <input class="mono" bind:value={qso.rcvd_locator} maxlength="10" /></label>{/if}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  {#snippet footer()}
    <button class="danger" onclick={() => ondelete(qso.id)}>Delete</button>
    <span class="spacer"></span>
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.85rem;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .row.three {
    grid-template-columns: 1fr 1fr 1fr;
  }
  h3 {
    margin: 6px 0 0;
    font-size: 0.8rem;
    text-transform: uppercase;
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
</style>
