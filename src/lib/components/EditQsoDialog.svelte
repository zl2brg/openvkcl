<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import {
    dateTimeText,
    isFutureQsoTime,
    parseDateTimeText,
    zoneLabel,
    type ContestDef,
    type Qso,
    type QsoRow,
  } from "$lib/api";

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
    name: r.name,
    qth: r.qth,
    comment: r.comment,
    qsl_sent: r.qsl_sent,
    qsl_rcvd: r.qsl_rcvd,
    lotw_sent: r.lotw_sent,
    lotw_rcvd: r.lotw_rcvd,
    eqsl_sent: r.eqsl_sent,
    eqsl_rcvd: r.eqsl_rcvd,
  });
  let timeText = $state(dateTimeText(r.time));
  let futureTime = $derived(isFutureQsoTime(parseDateTimeText(timeText)));
  let error = $state("");
  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let general = $derived(contest.general);
  // An imported band or mode the list doesn't have stays selectable.
  let bandChoices = $derived(contest.bands.includes(r.band) ? contest.bands : [...contest.bands, r.band]);
  let modeChoices = $derived(contest.modes.includes(r.mode) ? contest.modes : [...contest.modes, r.mode]);

  const QSL_STATUS: [string, string][] = [
    ["", "—"],
    ["Y", "Yes"],
    ["N", "No"],
    ["R", "Requested"],
    ["Q", "Queued"],
    ["I", "Ignore"],
  ];
  type QslField = "qsl_sent" | "qsl_rcvd" | "lotw_sent" | "lotw_rcvd" | "eqsl_sent" | "eqsl_rcvd";
  const QSL_ROWS: [string, QslField, QslField][] = [
    ["QSL card", "qsl_sent", "qsl_rcvd"],
    ["LoTW", "lotw_sent", "lotw_rcvd"],
    ["eQSL", "eqsl_sent", "eqsl_rcvd"],
  ];

  function save() {
    const t = parseDateTimeText(timeText);
    if (!t) {
      error = `Time must be YYYY-MM-DD HH:MM (${zoneLabel()}).`;
      return;
    }
    qso.time = t;
    onsave($state.snapshot(qso) as Qso);
  }
</script>

<Modal title={`Edit QSO with ${row.call}`} {onclose}>
  <div class="form">
    <div class="row">
      <label>Time ({zoneLabel()}) <input class="mono" bind:value={timeText} /></label>
      <label>Call <input class="mono" bind:value={qso.call} /></label>
    </div>
    {#if futureTime}
      <p class="warn">QSO time is in the future</p>
    {/if}
    <div class="row three">
      <label>
        Band
        <select bind:value={qso.band}>
          {#each bandChoices as b (b)}<option value={b}>{b}</option>{/each}
        </select>
      </label>
      <label>
        Mode
        <select bind:value={qso.mode}>
          {#each modeChoices as m (m)}<option value={m}>{m}</option>{/each}
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
      {#if hasLoc || general}
        <label>Locator <input class="mono" bind:value={qso.rcvd_locator} maxlength="10" /></label>
      {/if}
    </div>
    {#if general}
      <div class="row">
        <label>Name <input bind:value={qso.name} /></label>
        <label>QTH <input bind:value={qso.qth} /></label>
      </div>
      <label>Comment <input bind:value={qso.comment} /></label>
      <h3>QSL</h3>
      <div class="qsl">
        <span></span><span class="head">Sent</span><span class="head">Received</span>
        {#each QSL_ROWS as [label, sent, rcvd] (label)}
          <span>{label}</span>
          {#each [sent, rcvd] as field (field)}
            <select bind:value={qso[field]} aria-label="{label} {field.endsWith('sent') ? 'sent' : 'received'}">
              {#each QSL_STATUS as [v, text] (v)}<option value={v}>{text}</option>{/each}
            </select>
          {/each}
        {/each}
      </div>
    {/if}
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
  .qsl {
    display: grid;
    grid-template-columns: auto 1fr 1fr;
    gap: 6px 8px;
    align-items: center;
    font-size: 0.85rem;
  }
  .qsl .head {
    color: var(--muted);
    font-size: 0.8rem;
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
  .warn {
    margin: 0;
    color: var(--warn);
    font-size: 0.85rem;
  }
</style>
