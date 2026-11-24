<script lang="ts">
  import {
    dateText,
    hhmm,
    rangeTest,
    zoneLabel,
    type ContestDef,
    type DateRange,
    type QsoRow,
  } from "$lib/api";
  import Countdown from "./Countdown.svelte";

  let {
    rows,
    contest,
    highlight = "",
    callFilter = "",
    range = $bindable({ from: "", to: "" }),
    onedit,
    ondelete,
    ondone,
  }: {
    rows: QsoRow[];
    contest: ContestDef;
    /** Rows with this call are highlighted. */
    highlight?: string;
    /** The call being typed on the entry line: only its contacts are shown, as in N1MM. */
    callFilter?: string;
    /** Dates to show the log between, in general logs. */
    range?: DateRange;
    onedit: (row: QsoRow) => void;
    ondelete: (row: QsoRow) => void;
    /** Escape in an empty search box, to hand focus back. */
    ondone?: () => void;
  } = $props();

  let hasNr = $derived(contest.exchange.includes("serial"));
  let hasExch = $derived(contest.exchange.includes("number"));
  let hasLoc = $derived(contest.exchange.includes("locator"));
  let hasRework = $derived(contest.rework.kind !== "never");
  let general = $derived(contest.general);
  let newestFirst = $derived(rows.map((r, i) => ({ r, n: i + 1 })).reverse());

  let search = $state("");
  let searchBox: HTMLInputElement;
  let query = $derived(search.trim().toUpperCase());
  let dated = $derived(general && !!(range.from || range.to));

  /** A general log also finds countries and names, e.g. "japan". */
  function matches(r: QsoRow): boolean {
    if (r.call.toUpperCase().includes(query)) return true;
    return general && [r.country ?? "", r.name].some((x) => x.toUpperCase().includes(query));
  }

  // Numbered before filtering, so a match keeps its place in the log.
  // The search box wins; otherwise the call being typed narrows the log to it.
  let byCall = $derived(query ? "" : callFilter.trim().toUpperCase());

  let matching = $derived.by(() => {
    const inRange = dated ? rangeTest(range) : null;
    return newestFirst.filter(
      ({ r }) =>
        (!query || matches(r)) &&
        (!byCall || r.call.toUpperCase().includes(byCall)) &&
        (!inRange || inRange(r.time)),
    );
  });

  // Long logs show a page at a time: thousands of rows make the table slow.
  const PAGE = 500;
  let limit = $state(PAGE);
  $effect(() => {
    query;
    byCall;
    range.from;
    range.to;
    sort;
    limit = PAGE;
  });

  /** The right-click menu: which contact, where, and whether Delete was clicked once. */
  let menu = $state<{ row: QsoRow; x: number; y: number; confirming: boolean } | null>(null);
  let menuEl: HTMLDivElement | undefined = $state();

  function openMenu(e: MouseEvent, row: QsoRow) {
    e.preventDefault();
    // Keep it on screen near the window's right and bottom edges.
    menu = { row, x: Math.min(e.clientX, innerWidth - 200), y: Math.min(e.clientY, innerHeight - 90), confirming: false };
  }

  function closeMenu() {
    menu = null;
  }

  function menuEdit() {
    const row = menu!.row;
    closeMenu();
    onedit(row);
  }

  /** The first click asks, the second deletes. */
  function menuDelete() {
    if (!menu!.confirming) {
      menu!.confirming = true;
      return;
    }
    const row = menu!.row;
    closeMenu();
    ondelete(row);
  }

  function onWindowPointerdown(e: PointerEvent) {
    if (menu && !menuEl?.contains(e.target as Node)) closeMenu();
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (menu && e.key === "Escape") {
      e.preventDefault();
      closeMenu();
      return;
    }
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
    /** Fixed width, in general logs; columns without one share what's left. */
    width?: string;
    /** Left out in narrow windows. */
    optional?: boolean;
    /** What the column sorts by; null (no value) always sorts last. */
    value: (r: QsoRow, n: number) => SortValue;
  }

  /** Absolute re-work time for sorting; each cell owns its own countdown tick. */
  function reworkMs(r: QsoRow): number | null {
    return r.rework_at ? new Date(r.rework_at).getTime() : null;
  }

  const text = (s: string) => s.trim() || null;
  const exchange = (rst: string, nr: string, exch: string, loc: string) =>
    text([rst, hasNr ? nr : "", hasExch ? exch : "", hasLoc ? loc : ""].join(" "));

  function bandIndex(band: string): SortValue {
    const i = contest.bands.indexOf(band);
    return i < 0 ? text(band) : i;
  }

  function countryTitle(r: QsoRow): string {
    if (!r.country) return "";
    const zones = [r.cont, r.cqz ? `CQ ${r.cqz}` : "", r.ituz ? `ITU ${r.ituz}` : ""].filter(Boolean).join(" · ");
    return `${r.country} (DXCC ${r.dxcc})${zones ? ` · ${zones}` : ""}`;
  }

  let timeLabel = $derived(zoneLabel() === "UTC" ? "UTC" : "Local");
  let generalColumns = $derived<Column[]>([
    { key: "n", label: "#", num: true, width: "4.2rem", value: (_, n) => n },
    { key: "time", label: timeLabel, width: "10rem", value: (r) => new Date(r.time).getTime() },
    { key: "band", label: "Band", width: "4.4rem", value: (r) => bandIndex(r.band) },
    { key: "mode", label: "Mode", width: "4.6rem", value: (r) => text(r.mode) },
    { key: "call", label: "Call", width: "7rem", value: (r) => text(r.call) },
    { key: "country", label: "Country", value: (r) => text(r.country ?? "") },
    { key: "sent", label: "Sent", width: "4.4rem", value: (r) => text(r.sent_rst) },
    { key: "rcvd", label: "Rcvd", width: "4.4rem", value: (r) => text(r.rcvd_rst) },
    { key: "name", label: "Name", value: (r) => text(r.name) },
    { key: "comment", label: "Comment", optional: true, value: (r) => text(r.comment) },
  ]);

  let contestColumns = $derived<Column[]>([
    { key: "n", label: "#", num: true, value: (_, n) => n },
    { key: "time", label: timeLabel, value: (r) => new Date(r.time).getTime() },
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
  // Below this window width the Comment column is left out, so Country and Name keep room.
  const WIDE = 1250;
  let windowWidth = $state(WIDE);
  let showComment = $derived(windowWidth >= WIDE);
  let columns = $derived(
    general ? generalColumns.filter((c) => !c.optional || showComment) : contestColumns,
  );

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

<svelte:window
  onkeydown={onWindowKeydown}
  onpointerdown={onWindowPointerdown}
  onblur={closeMenu}
  onresize={closeMenu}
  bind:innerWidth={windowWidth}
/>

<div class="log">
  <div class="search">
    <input
      bind:this={searchBox}
      bind:value={search}
      onkeydown={onSearchKeydown}
      class="mono"
      type="search"
      placeholder="Search log"
      aria-label={general ? "Search the log by callsign, country or name" : "Search the log by callsign"}
      title="Find a {general ? 'callsign, country or name' : 'callsign'} in the log (Ctrl+F / ⌘F). Esc clears."
      spellcheck="false"
      autocomplete="off"
    />
    {#if general}
      <label class="date">
        From
        <input type="date" bind:value={range.from} max={range.to || undefined} title="Show contacts from this date ({zoneLabel()})" />
      </label>
      <label class="date">
        To
        <input type="date" bind:value={range.to} min={range.from || undefined} title="Show contacts up to this date ({zoneLabel()})" />
      </label>
      {#if dated}
        <button class="clear" onclick={() => (range = { from: "", to: "" })} title="Show all dates">Clear dates</button>
      {/if}
    {/if}
    {#if byCall}
      <span class="count" title="Contacts with the call being typed; clear the call to see the whole log">
        {matching.length.toLocaleString()} with {byCall}
      </span>
    {:else if query || dated}
      <span class="count">{matching.length.toLocaleString()} of {rows.length.toLocaleString()}</span>
    {/if}
  </div>
  <div class="wrap" onscroll={closeMenu}>
    <table class:fit={general}>
      {#if general}
        <colgroup>
          {#each columns as c (c.key)}<col style:width={c.width} />{/each}
        </colgroup>
      {/if}
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
        {#each shown.slice(0, limit) as { r, n } (r.id)}
          {@const s = status(r)}
          <tr
            class:dim={r.dupe || !!r.invalid}
            class:hl={highlight && r.call === highlight}
            class:menu-row={menu?.row.id === r.id}
            ondblclick={() => onedit(r)}
            oncontextmenu={(e) => openMenu(e, r)}
            title="Double-click to edit · right-click for more"
          >
            <td class="num muted">{n}</td>
            {#if general}
              <td class="mono">{dateText(r.time)} {hhmm(r.time)}</td>
              <td>{r.band}</td>
              <td title={r.mode}>{r.mode}</td>
              <td class="mono call" title={r.call}>{r.call}</td>
              <td title={countryTitle(r)}>
                {#if r.new_dxcc}<span class="new" title="First contact with {r.country}">NEW</span>{/if}{r.country ?? ""}
              </td>
              <td class="mono">{r.sent_rst}</td>
              <td class="mono">{r.rcvd_rst}</td>
              <td title={r.name}>{r.name}</td>
              {#if showComment}<td class="muted" title={r.comment}>{r.comment}</td>{/if}
            {:else}
              <td class="mono">{hhmm(r.time)}</td>
              <td>{r.band}</td>
              <td>{r.mode}</td>
              <td class="mono call">{r.call}</td>
              {#if hasRework}
                <td>
                  {#if r.rework_at}
                    <Countdown at={r.rework_at} title={reworkTitle(r)} />
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
            {/if}
          </tr>
        {:else}
          <tr>
            <td colspan={columns.length} class="empty">
              {#if byCall && rows.length}
                No contacts with {byCall} yet.
              {:else if query && rows.length}
                No contacts match “{query}”{dated ? " between those dates" : ""}.
              {:else if dated && rows.length}
                No contacts between those dates.
              {:else}
                No contacts yet. Type a callsign above and press Enter.
              {/if}
            </td>
          </tr>
        {/each}
        {#if shown.length > limit}
          <tr>
            <td colspan={columns.length} class="more">
              Showing {limit.toLocaleString()} of {shown.length.toLocaleString()} contacts.
              <button onclick={() => (limit += PAGE)}>Show {PAGE} more</button>
              <button onclick={() => (limit = shown.length)}>Show all</button>
            </td>
          </tr>
        {/if}
      </tbody>
    </table>
  </div>
</div>

{#if menu}
  <div class="menu" role="menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px">
    <button role="menuitem" onclick={menuEdit}>Edit {menu.row.call}…</button>
    <button role="menuitem" class="danger" class:confirming={menu.confirming} onclick={menuDelete}>
      {menu.confirming ? `Delete ${menu.row.call}? Click to confirm` : `Delete ${menu.row.call}`}
    </button>
  </div>
{/if}

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
  .date {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .clear {
    font-size: 0.85rem;
    padding: 3px 8px;
  }
  /* General logs: fixed column widths so the table always fits, long text cut short. */
  table.fit {
    table-layout: fixed;
  }
  table.fit th,
  table.fit td {
    padding-left: 6px;
    padding-right: 6px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  table.fit td.more {
    white-space: normal;
  }
  .new {
    margin-right: 6px;
    padding: 0 5px;
    border: 1px solid var(--accent);
    border-radius: 8px;
    color: var(--accent);
    font-size: 0.75rem;
    font-weight: 700;
  }
  .more {
    text-align: center;
    color: var(--muted);
    padding: 10px;
  }
  .more button {
    margin-left: 8px;
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
  tr.hl td,
  tr.menu-row td {
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
  .menu {
    position: fixed;
    z-index: 20;
    min-width: 190px;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.35);
  }
  .menu button {
    all: unset;
    padding: 5px 10px;
    border-radius: 4px;
    font-size: 0.88rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--hover);
  }
  .menu .danger {
    color: var(--bad);
  }
  .menu .danger.confirming {
    background: var(--bad);
    color: var(--bg);
    font-weight: 600;
  }
</style>
