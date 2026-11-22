<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { errorText, rangeTest, zoneLabel, type DateRange, type ImportSummary, type QsoRow } from "$lib/api";

  let {
    kind,
    file = "",
    general,
    rows,
    initial,
    onimport,
    onexport,
    onclose,
  }: {
    kind: "import" | "export";
    /** The file being imported. */
    file?: string;
    general: boolean;
    /** The log's contacts, to count what an export will hold. */
    rows: QsoRow[];
    initial: DateRange;
    onimport: (range: DateRange) => Promise<ImportSummary>;
    /** Resolves to false if the save was cancelled. */
    onexport: (range: DateRange) => Promise<boolean>;
    onclose: () => void;
  } = $props();

  let range = $state<DateRange>(untrack(() => ({ ...initial })));
  let busy = $state(false);
  let error = $state("");
  let summary = $state<ImportSummary | null>(null);

  let count = $derived.by(() => {
    const inRange = rangeTest(range);
    return rows.filter((r) => inRange(r.time)).length;
  });
  let fileName = $derived(file.split(/[\\/]/).pop() ?? file);

  async function go() {
    if (range.from && range.to && range.from > range.to) {
      error = "The From date is after the To date.";
      return;
    }
    busy = true;
    error = "";
    try {
      if (kind === "import") {
        summary = await onimport($state.snapshot(range));
      } else if (await onexport($state.snapshot(range))) {
        onclose();
      }
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function plural(n: number, one: string, many = `${one}s`) {
    return `${n.toLocaleString()} ${n === 1 ? one : many}`;
  }
</script>

<Modal title={kind === "import" ? "Import ADIF" : "Export ADIF"} {onclose}>
  <div class="form">
    {#if kind === "import"}
      <p>Add the contacts in <strong title={file}>{fileName}</strong> to this log.</p>
      <p class="hint">
        Contacts already in the log (same call, minute, band and mode) are skipped{general
          ? ""
          : ", as are contacts outside the contest period or on bands and modes the contest doesn't use"}.
      </p>
    {:else}
      <p>Save contacts from this log as an ADIF (.adi) file, for LoTW, QRZ.com, Club Log or another logger.</p>
    {/if}

    <h3>Dates ({zoneLabel()})</h3>
    <div class="row">
      <label>From <input type="date" bind:value={range.from} disabled={busy || !!summary} /></label>
      <label>To <input type="date" bind:value={range.to} disabled={busy || !!summary} /></label>
    </div>
    <p class="hint">Leave a date empty for no limit, e.g. From 2026-01-01 for this year's contacts only.</p>

    {#if kind === "export"}
      <p>{plural(count, "contact")} to export.</p>
    {/if}

    {#if summary}
      {@const s = summary}
      <div class="result">
        <p class="ok"><strong>Imported {plural(s.imported, "contact")}.</strong></p>
        {#if s.duplicates}<p>{plural(s.duplicates, "contact")} already in the log</p>{/if}
        {#if s.out_of_range}<p>{plural(s.out_of_range, "contact")} outside the dates</p>{/if}
        {#if s.not_in_contest}<p>{plural(s.not_in_contest, "contact")} outside the contest period, bands or modes</p>{/if}
        {#if s.unreadable}<p class="warn">{plural(s.unreadable, "record")} missing a callsign, date, time or band</p>{/if}
      </div>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>

  {#snippet footer()}
    {#if summary}
      <button class="primary" onclick={onclose}>Done</button>
    {:else}
      <button onclick={onclose}>Cancel</button>
      <button class="primary" onclick={go} disabled={busy || (kind === "export" && !count)}>
        {busy ? "Working…" : kind === "import" ? "Import" : "Export…"}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 26rem;
  }
  p {
    margin: 0;
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
  h3 {
    margin: 6px 0 0;
    font-size: 0.8rem;
    text-transform: uppercase;
    color: var(--muted);
  }
  .hint {
    font-size: 0.85rem;
    color: var(--muted);
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .ok {
    color: var(--ok);
  }
  .warn {
    color: var(--warn);
  }
</style>
