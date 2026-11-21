<script lang="ts">
  import { countdownText, hhmm, zoneLabel, type ContestDef, type QsoRow } from "$lib/api";

  let {
    rows,
    contest,
    highlight = "",
    now,
    onedit,
  }: {
    rows: QsoRow[];
    contest: ContestDef;
    /** Ticks every second to drive the re-work countdowns. */
    now: Date;
    /** Rows with this call are highlighted. */
    highlight?: string;
    onedit: (row: QsoRow) => void;
  } = $props();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let hasRework = $derived(contest.rework.kind !== "never");
  let newestFirst = $derived(rows.map((r, i) => ({ r, n: i + 1 })).reverse());

  /** Sorting by the "Again in" column: soonest first, latest first, or the log's own order. */
  let reworkSort = $state<"asc" | "desc" | null>(null);

  function cycleReworkSort() {
    reworkSort = reworkSort === null ? "asc" : reworkSort === "asc" ? "desc" : null;
  }

  /** When a running countdown ends, or null when there is none. */
  function reworkMs(r: QsoRow): number | null {
    const t = r.rework_at ? new Date(r.rework_at).getTime() : NaN;
    return t > now.getTime() ? t : null;
  }

  let shown = $derived.by(() => {
    if (!reworkSort || !hasRework) return newestFirst;
    const dir = reworkSort === "asc" ? 1 : -1;
    // Stable sort, so rows without a countdown keep the newest-first order at the bottom.
    return [...newestFirst].sort((a, b) => {
      const ta = reworkMs(a.r);
      const tb = reworkMs(b.r);
      if (ta === null || tb === null) return (ta === null ? 1 : 0) - (tb === null ? 1 : 0);
      return (ta - tb) * dir;
    });
  });

  function status(r: QsoRow): { text: string; cls: string; title: string } {
    if (r.invalid) return { text: "INVALID", cls: "bad", title: r.invalid };
    if (r.dupe) return { text: "DUPE", cls: "bad", title: "Duplicate contact" };
    if (r.warning) return { text: "CHECK", cls: "warn", title: r.warning };
    if (r.new_mult) return { text: `MULT ${r.mult}`, cls: "mult", title: "New multiplier" };
    return { text: "", cls: "", title: "" };
  }

  function reworkTitle(r: QsoRow) {
    const per = contest.rework.per.includes("band") ? ` on ${r.band} ${r.mode}` : "";
    return `${r.call} can be worked again${per} at ${hhmm(r.rework_at!)}`;
  }

  function points(p: number) {
    return p % 1 ? p.toFixed(1) : p.toFixed(0);
  }
</script>

<div class="wrap">
  <table>
    <thead>
      <tr>
        <th class="num">#</th>
        <th>{zoneLabel() === "UTC" ? "UTC" : "Local"}</th>
        <th>Band</th>
        <th>Mode</th>
        <th>Call</th>
        {#if hasRework}
          <th aria-sort={reworkSort === "asc" ? "ascending" : reworkSort === "desc" ? "descending" : "none"}>
            <button
              class="sort"
              onclick={cycleReworkSort}
              title={reworkSort === "asc"
                ? "Sorted soonest first. Click for latest first"
                : reworkSort === "desc"
                  ? "Sorted latest first. Click for log order"
                  : "Sort by when each station can be worked again"}
            >
              Again in<span class="arrow">{reworkSort === "asc" ? "▲" : reworkSort === "desc" ? "▼" : "↕"}</span>
            </button>
          </th>
        {/if}
        <th>Sent</th>
        <th>Rcvd</th>
        {#if hasLoc}<th class="num">km</th>{/if}
        <th class="num">Pts</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each shown as { r, n } (r.id)}
        {@const s = status(r)}
        <tr
          class:dim={r.dupe || !!r.invalid}
          class:hl={highlight && r.call === highlight}
          ondblclick={() => onedit(r)}
          title="Double-click to edit"
        >
          <td class="num muted">{n}</td>
          <td class="mono">{hhmm(r.time)}</td>
          <td>{r.band}</td>
          <td>{r.mode}</td>
          <td class="mono call">{r.call}</td>
          {#if hasRework}
            <td>
              {#if r.rework_at}
                {@const left = countdownText(r.rework_at, now)}
                {#if left}<span class="rework" title={reworkTitle(r)}>⏱ {left}</span>{/if}
              {/if}
            </td>
          {/if}
          <td class="mono">
            {r.sent_rst}{hasNr ? ` ${r.sent_nr}` : ""}{hasExch ? ` ${r.sent_exch}` : ""}{hasLoc
              ? ` ${r.sent_locator}`
              : ""}
          </td>
          <td class="mono">
            {r.rcvd_rst}{hasNr ? ` ${r.rcvd_nr}` : ""}{hasExch ? ` ${r.rcvd_exch}` : ""}{hasLoc
              ? ` ${r.rcvd_locator}`
              : ""}
          </td>
          {#if hasLoc}<td class="num">{r.distance_km != null ? r.distance_km.toFixed(0) : ""}</td>{/if}
          <td class="num">{points(r.points)}</td>
          <td class={s.cls} title={s.title}>{s.text}</td>
        </tr>
      {:else}
        <tr><td colspan="11" class="empty">No contacts yet. Type a callsign above and press Enter.</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .wrap {
    overflow: auto;
    height: 100%;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel-2);
    text-align: left;
    font-weight: 600;
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--muted);
    padding: 6px 8px;
  }
  td {
    padding: 3px 8px;
    border-top: 1px solid var(--border);
    white-space: nowrap;
  }
  tr:hover td {
    background: var(--hover);
    cursor: default;
  }
  tr.hl td {
    background: var(--highlight);
  }
  tr.dim td {
    opacity: 0.55;
  }
  .num {
    text-align: right;
  }
  .call {
    font-weight: 600;
  }
  .sort {
    all: unset;
    cursor: pointer;
    font: inherit;
    color: inherit;
    text-transform: inherit;
    white-space: nowrap;
  }
  .sort:hover {
    color: var(--fg);
  }
  .sort:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .arrow {
    margin-left: 4px;
    opacity: 0.8;
  }
  th[aria-sort="none"] .arrow {
    opacity: 0.4;
  }
  .rework {
    display: inline-block;
    min-width: 6.5em;
    text-align: center;
    padding: 1px 7px;
    border: 1px solid var(--warn);
    border-radius: 10px;
    color: var(--warn);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--bad);
    font-weight: 600;
  }
  .warn {
    color: var(--warn);
  }
  .mult {
    color: var(--accent);
  }
  .empty {
    text-align: center;
    color: var(--muted);
    padding: 24px;
  }
</style>
