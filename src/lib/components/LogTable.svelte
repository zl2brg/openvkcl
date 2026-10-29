<script lang="ts">
  import { hhmm, type ContestDef, type QsoRow } from "$lib/api";

  let {
    rows,
    contest,
    highlight = "",
    onedit,
  }: {
    rows: QsoRow[];
    contest: ContestDef;
    /** Rows with this call are highlighted. */
    highlight?: string;
    onedit: (row: QsoRow) => void;
  } = $props();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let newestFirst = $derived(rows.map((r, i) => ({ r, n: i + 1 })).reverse());

  function status(r: QsoRow): { text: string; cls: string; title: string } {
    if (r.invalid) return { text: "INVALID", cls: "bad", title: r.invalid };
    if (r.dupe) return { text: "DUPE", cls: "bad", title: "Duplicate contact" };
    if (r.warning) return { text: "CHECK", cls: "warn", title: r.warning };
    if (r.new_mult) return { text: `MULT ${r.mult}`, cls: "mult", title: "New multiplier" };
    return { text: "", cls: "", title: "" };
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
        <th>Time</th>
        <th>Band</th>
        <th>Mode</th>
        <th>Call</th>
        <th>Sent</th>
        <th>Rcvd</th>
        {#if hasLoc}<th class="num">km</th>{/if}
        <th class="num">Pts</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each newestFirst as { r, n } (r.id)}
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
        <tr><td colspan="10" class="empty">No contacts yet. Type a callsign above and press Enter.</td></tr>
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
    font-size: 0.75rem;
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
