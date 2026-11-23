<script lang="ts">
  import { hhmm, type ClusterStatus, type QsoRow, type Spot } from "$lib/api";

  let {
    spots,
    status,
    qsos,
    onspot,
  }: {
    /** Recent spots, oldest first. */
    spots: Spot[];
    status: ClusterStatus;
    /** The whole log, to tell which countries are still needed. */
    qsos: QsoRow[];
    onspot: (spot: Spot) => void;
  } = $props();

  /** Spots older than this drop off the list. */
  const MAX_AGE_MS = 30 * 60_000;
  const MAX_SHOWN = 60;
  const FILTER_KEY = "openvkcl.cluster.filter";

  type Filter = "country" | "band";
  function savedFilter(): Filter {
    try {
      return localStorage.getItem(FILTER_KEY) === "band" ? "band" : "country";
    } catch {
      return "country";
    }
  }
  let filter = $state<Filter>(savedFilter());
  $effect(() => {
    try {
      localStorage.setItem(FILTER_KEY, filter);
    } catch {
      // Not stored: the choice lasts until the app closes.
    }
  });

  // Ages spots off the list without waiting for a new one.
  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  let worked = $derived(new Set(qsos.flatMap((q) => (q.dxcc != null ? [q.dxcc] : []))));
  let workedBand = $derived(new Set(qsos.flatMap((q) => (q.dxcc != null ? [`${q.dxcc}|${q.band}`] : []))));

  /** Needed spots, newest first: one per call and band. */
  let wanted = $derived.by(() => {
    const out: (Spot & { newCountry: boolean })[] = [];
    const seen = new Set<string>();
    for (let i = spots.length - 1; i >= 0 && out.length < MAX_SHOWN; i--) {
      const s = spots[i];
      if (s.dxcc == null || now - new Date(s.time).getTime() > MAX_AGE_MS) continue;
      const newCountry = !worked.has(s.dxcc);
      const newBand = !!s.band && !workedBand.has(`${s.dxcc}|${s.band}`);
      if (!newCountry && !(filter === "band" && newBand)) continue;
      const key = `${s.call}|${s.band}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ ...s, newCountry });
    }
    return out;
  });

  let unknownCountries = $derived(spots.length > 0 && spots.every((s) => s.dxcc == null));
</script>

<section>
  <div class="head">
    <h3>DX cluster</h3>
    <span class="dot" class:on={status.connected} title={status.error ?? status.node}></span>
    <select bind:value={filter} aria-label="Which spots to show">
      <option value="country">New countries</option>
      <option value="band">New countries and bands</option>
    </select>
  </div>

  {#if !status.node}
    <p class="muted">Off. Turn it on under Lookups…</p>
  {:else if status.error}
    <p class="bad">{status.error}</p>
  {:else if !status.connected}
    <p class="muted">Connecting to {status.node}…</p>
  {:else if unknownCountries}
    <p class="muted">Countries unknown: download the country file under Lookups…</p>
  {:else if !wanted.length}
    <p class="muted">No spots of {filter === "band" ? "new countries or bands" : "new countries"} in the last 30 minutes.</p>
  {/if}

  {#if wanted.length}
    <ul>
      {#each wanted as s (`${s.call}|${s.band}`)}
        <li>
          <button
            onclick={() => onspot(s)}
            title="{s.comment ? `${s.comment} · ` : ''}spotted by {s.spotter} at {hhmm(s.time)}. Click to log."
          >
            <span class="mono freq">{s.freq_khz.toFixed(1)}</span>
            <span class="mono call">{s.call}</span>
            <span class="country">{s.country}</span>
            <span class="tag" class:band={!s.newCountry}>{s.newCountry ? "NEW" : s.band}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  h3 {
    margin: 0;
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }
  .dot.on {
    background: var(--ok);
  }
  select {
    margin-left: auto;
    font-size: 0.75rem;
    padding: 1px 4px;
  }
  p {
    margin: 2px 0;
    font-size: 0.85rem;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--bad);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
  }
  li button {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    display: grid;
    grid-template-columns: 4.2em 6.6em 1fr auto;
    gap: 6px;
    align-items: center;
    padding: 2px 4px;
    border-radius: 3px;
    font-size: 0.85rem;
    cursor: pointer;
  }
  li button:hover,
  li button:focus-visible {
    background: var(--hover);
  }
  .freq {
    text-align: right;
    color: var(--muted);
  }
  .call {
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .country {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    padding: 0 5px;
    border: 1px solid var(--accent);
    border-radius: 8px;
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 700;
  }
  .tag.band {
    border-color: var(--warn);
    color: var(--warn);
  }
</style>
