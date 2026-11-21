<script lang="ts">
  import { countdownText, hhmm, zoneLabel, type ContestDef, type QsoRow } from "$lib/api";

  let {
    rows,
    contest,
    highlight = "",
    now,
    onedit,
    ondone,
  }: {
    rows: QsoRow[];
    contest: ContestDef;
    /** Ticks every second to drive the re-work countdowns. */
    now: Date;
    /** Rows with this call are highlighted. */
    highlight?: string;
    onedit: (row: QsoRow) => void;
    /** Escape in an empty search box, to hand focus back. */
    ondone?: () => void;
  } = $props();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let hasRework = $derived(contest.rework.kind !== "never");
  let newestFirst = $derived(rows.map((r, i) => ({ r, n: i + 1 })).reverse());

  let search = $state("");
  let searchBox: HTMLInputElement;
  let query = $derived(search.trim().toUpperCase());
  // Numbered before filtering, so a match keeps its place in the log.
  let matching = $derived(query ? newestFirst.filter(({ r }) => r.call.toUpperCase().includes(query)) : newestFirst);

  function onWindowKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f" && !document.querySelector("[role=dialog]")) {
      e.preventDefault();
      searchBox.focus();
      searchBox.select();
    }
  }

  /** Escape clears the search, and a second Escape goes back to the entry line. */
  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    if (search) search = "";
    else ondone?.();
  }

  type SortValue = string | number | null;
  interface Column {
    key: string;
    label: string;
    num?: boolean;
    /** What the column sorts by; null (no value) always sorts last. */
    value: (r: QsoRow, n: number) => SortValue;
  }

  /** When a running countdown ends, or null when there is none. */
  function reworkMs(r: QsoRow): number | null {
    const t = r.rework_at ? new Date(r.rework_at).getTime() : NaN;
    return t > now.getTime() ? t : null;
  }

  const text = (s: string) => s.trim() || null;
  const exchange = (rst: string, nr: string, exch: string, loc: string) =>
    text([rst, hasNr ? nr : "", hasExch ? exch : "", hasLoc ? loc : ""].join(" "));

  function bandIndex(band: string): SortValue {
    const i = contest.bands.indexOf(band);
    return i < 0 ? text(band) : i;
  }

  let columns = $derived<Column[]>([
    { key: "n", label: "#", num: true, value: (_, n) => n },
    { key: "time", label: zoneLabel() === "UTC" ? "UTC" : "Local", value: (r) => new Date(r.time).getTime() },
    // In the contest's band order (160m before 80m), not alphabetical.
    { key: "band", label: "Band", value: (r) => bandIndex(r.band) },
    { key: "mode", label: "Mode", value: (r) => text(r.mode) },
    { key: "call", label: "Call", value: (r) => text(r.call) },
    ...(hasRework ? [{ key: "rework", label: "Again in", value: reworkMs }] : []),
    { key: "sent", label: "Sent", value: (r) => exchange(r.sent_rst, r.sent_nr, r.sent_exch, r.sent_locator) },
    { key: "rcvd", label: "Rcvd", value: (r) => exchange(r.rcvd_rst, r.rcvd_nr, r.rcvd_exch, r.rcvd_locator) },
    ...(hasLoc ? [{ key: "km", label: "km", num: true, value: (r: QsoRow) => r.distance_km }] : []),
    { key: "pts", label: "Pts", num: true, value: (r) => r.points },
    { key: "status", label: "Status", value: (r) => text(status(r).text) },
  ]);

  /** The sorted column and direction; null keeps the log's newest-first order. */
  let sort = $state<{ key: string; dir: 1 | -1 } | null>(null);

  /** Each click on a header goes ascending, descending, then back to log order. */
  function toggleSort(key: string) {
    if (sort?.key !== key) sort = { key, dir: 1 };
    else if (sort.dir === 1) sort = { key, dir: -1 };
    else sort = null;
  }

  // Numbers inside text compare as numbers, so "59 9" sorts before "59 10".
  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

  function compare(a: SortValue, b: SortValue): number {
    if (typeof a === "number" && typeof b === "number") return a - b;
    if (typeof a === "number") return -1;
    if (typeof b === "number") return 1;
    return collator.compare(a ?? "", b ?? "");
  }

  let shown = $derived.by(() => {
    const col = sort && columns.find((c) => c.key === sort!.key);
    if (!sort || !col) return matching;
    const dir = sort.dir;
    // A stable sort, so equal values and rows with no value keep the newest-first order.
    return [...matching].sort((a, b) => {
      const va = col.value(a.r, a.n);
      const vb = col.value(b.r, b.n);
      if (va === null || vb === null) return (va === null ? 1 : 0) - (vb === null ? 1 : 0);
      return compare(va, vb) * dir;
    });
  });

  function sortTitle(c: Column) {
    if (sort?.key !== c.key) return `Sort by ${c.label}`;
    return sort.dir === 1
      ? `Sorted by ${c.label}, ascending. Click for descending`
      : `Sorted by ${c.label}, descending. Click for log order`;
  }

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

<svelte:window onkeydown={onWindowKeydown} />

<div class="log">
  <div class="search">
    <input
      bind:this={searchBox}
      bind:value={search}
      onkeydown={onSearchKeydown}
      class="mono"
      type="search"
      placeholder="Search log"
      aria-label="Search the log by callsign"
      title="Find a callsign in the log (Ctrl+F / ⌘F). Esc clears."
      spellcheck="false"
      autocomplete="off"
    />
    {#if query}
      <span class="count">{matching.length} of {rows.length}</span>
    {/if}
  </div>
  <div class="wrap">
    <table>
      <thead>
        <tr>
          {#each columns as c (c.key)}
            {@const dir = sort?.key === c.key ? sort.dir : 0}
            <th class:num={c.num} aria-sort={dir === 1 ? "ascending" : dir === -1 ? "descending" : "none"}>
              <button class="sort" onclick={() => toggleSort(c.key)} title={sortTitle(c)}>
                {c.label}<span class="arrow">{dir === 1 ? "▲" : dir === -1 ? "▼" : "↕"}</span>
              </button>
            </th>
          {/each}
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
          <tr>
            <td colspan={columns.length} class="empty">
              {#if query && rows.length}
                No contacts match “{query}”.
              {:else}
                No contacts yet. Type a callsign above and press Enter.
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .log {
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: 100%;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search input {
    width: 14em;
    text-transform: uppercase;
  }
  .search input::placeholder {
    text-transform: none;
  }
  .count {
    font-size: 0.85rem;
    color: var(--muted);
  }
  .wrap {
    overflow: auto;
    flex: 1;
    min-height: 0;
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
  .sort:hover,
  th:not([aria-sort="none"]) .sort {
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
