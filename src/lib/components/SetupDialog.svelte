<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { isLocator, pad, type ContestDef, type LogSetup } from "$lib/api";

  let {
    contests,
    initial,
    isNew,
    onsave,
    onclose,
  }: {
    contests: ContestDef[];
    initial: LogSetup;
    isNew: boolean;
    onsave: (setup: LogSetup) => void;
    onclose: () => void;
  } = $props();

  let setup = $state<LogSetup>(untrack(() => $state.snapshot(initial)) as LogSetup);
  const startDate0 = new Date(setup.start);
  let startDate = $state(
    `${startDate0.getUTCFullYear()}-${pad(startDate0.getUTCMonth() + 1)}-${pad(startDate0.getUTCDate())}`,
  );
  let startTime = $state(`${pad(startDate0.getUTCHours())}:${pad(startDate0.getUTCMinutes())}`);
  let error = $state("");

  let contest = $derived(contests.find((c) => c.id === setup.contest_id));

  const offsets = Array.from({ length: 53 }, (_, i) => (i - 24) * 30);
  function offsetLabel(m: number) {
    const sign = m < 0 ? "-" : "+";
    const a = Math.abs(m);
    return `UTC${sign}${pad(Math.floor(a / 60))}:${pad(a % 60)}`;
  }

  function contestChanged() {
    const c = contests.find((x) => x.id === setup.contest_id);
    if (!c) return;
    startTime = c.default_start_utc;
    setup.serial_per_band = c.serial_per_band;
    setup.categories = Object.fromEntries(c.categories.map((f) => [f.key, f.default]));
  }

  function save() {
    error = "";
    const start = new Date(`${startDate}T${startTime}:00Z`);
    if (isNaN(start.getTime())) {
      error = "Enter a valid start date and time.";
      return;
    }
    if (!setup.station.callsign.trim()) {
      error = "Enter your callsign.";
      return;
    }
    if (contest?.exchange.includes("number") && !/^\d+$/.test(setup.sent_number.trim())) {
      error = `Enter your ${contest.number_label.toLowerCase()} to send.`;
      return;
    }
    const loc = setup.station.locator.trim();
    if (contest?.exchange.includes("locator") && (!isLocator(loc) || loc.length < 6)) {
      error = "Enter your grid locator: 6 characters, or 8 or 10 for more precision.";
      return;
    }
    setup.station.locator = loc;
    setup.start = start.toISOString();
    setup.station.callsign = setup.station.callsign.trim().toUpperCase();
    setup.station.operators = setup.station.operators.trim().toUpperCase();
    setup.sent_number = setup.sent_number.trim();
    onsave($state.snapshot(setup) as LogSetup);
  }
</script>

<Modal title={isNew ? "New contest log" : "Contest setup"} {onclose} wide>
  <div class="grid">
    <section>
      <h3>Contest</h3>
      <label>
        Contest
        <select bind:value={setup.contest_id} onchange={contestChanged} disabled={!isNew}>
          {#each contests as c (c.id)}
            <option value={c.id}>{c.name}</option>
          {/each}
        </select>
      </label>
      {#if contest?.rules_url}
        <p class="hint">Rules: {contest.rules_url}</p>
      {/if}
      <div class="row">
        <label>
          Start date (UTC)
          <input type="date" bind:value={startDate} />
        </label>
        <label>
          Start time (UTC)
          <input type="time" bind:value={startTime} />
        </label>
      </div>
      <label>
        Your local time zone
        <select bind:value={setup.utc_offset_minutes}>
          {#each offsets as o (o)}
            <option value={o}>{offsetLabel(o)}</option>
          {/each}
        </select>
      </label>

      <h3>Exchange</h3>
      <div class="row">
        <label>
          Callsign
          <input class="mono" bind:value={setup.station.callsign} autocapitalize="characters" />
        </label>
        <label>
          Grid locator
          <input class="mono" bind:value={setup.station.locator} maxlength="10" />
        </label>
      </div>
      {#if contest?.exchange.includes("number")}
        <label>
          {contest.number_label} (sent)
          <input class="mono" bind:value={setup.sent_number} inputmode="numeric" />
        </label>
      {/if}
      {#if contest?.exchange.includes("serial")}
        <label class="check">
          <input type="checkbox" bind:checked={setup.serial_per_band} />
          Separate serial numbers on each band
        </label>
      {/if}

      <h3>Categories</h3>
      {#each contest?.categories ?? [] as cat (cat.key)}
        <label class:plain={!!cat.label}>
          {cat.label || cat.key.replace("CATEGORY-", "").toLowerCase()}
          <select bind:value={setup.categories[cat.key]}>
            {#each cat.options as opt (opt)}
              <option value={opt}>{opt || "(none)"}</option>
            {/each}
          </select>
        </label>
      {/each}
    </section>

    <section>
      <h3>Station details (Cabrillo header)</h3>
      <label>Operators <input class="mono" bind:value={setup.station.operators} placeholder="VK3ABC VK3XYZ" /></label>
      <label>Name <input bind:value={setup.station.name} /></label>
      <label>Email <input type="email" bind:value={setup.station.email} /></label>
      <label>Club <input bind:value={setup.station.club} /></label>
      <label>Address <input bind:value={setup.station.address} /></label>
      <div class="row">
        <label>City <input bind:value={setup.station.city} /></label>
        <label>State <input bind:value={setup.station.state} /></label>
      </div>
      <div class="row">
        <label>Postcode <input bind:value={setup.station.postcode} /></label>
        <label>Country <input bind:value={setup.station.country} /></label>
      </div>
      <label>Location <input bind:value={setup.location} placeholder="e.g. VIC" /></label>
      <label>Soapbox <textarea rows="3" bind:value={setup.soapbox}></textarea></label>
    </section>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={save}>{isNew ? "Create log…" : "Save"}</button>
  {/snippet}
</Modal>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }
  h3:first-child {
    margin-top: 0;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.85rem;
    text-transform: capitalize;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    text-transform: none;
  }
  label.plain {
    text-transform: none;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
    word-break: break-all;
  }
</style>
