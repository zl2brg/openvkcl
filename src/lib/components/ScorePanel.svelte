<script lang="ts">
  import {
    dateText,
    dateTimeText,
    hhmm,
    isConfirmed,
    modeClass,
    zoneLabel,
    type CheckResult,
    type ClusterStatus,
    type DateRange,
    type LogView,
    type QsoRow,
    type Spot,
  } from "$lib/api";
  import ClusterPanel from "./ClusterPanel.svelte";
  import ContestTiming from "./ContestTiming.svelte";

  let {
    view,
    check,
    rows,
    range,
    spots,
    clusterStatus,
    onspot,
  }: {
    view: LogView;
    check: CheckResult | null;
    /** In general logs, the contacts within `range`. */
    rows: QsoRow[];
    range: DateRange;
    /** DX cluster spots and connection, shown in general logs. */
    spots: Spot[];
    clusterStatus: ClusterStatus;
    onspot: (spot: Spot) => void;
  } = $props();

  let general = $derived(view.contest.general);

  interface Tally {
    qsos: number;
    worked: Set<number>;
    confirmed: Set<number>;
  }

  const tally = (): Tally => ({ qsos: 0, worked: new Set(), confirmed: new Set() });

  /** Countries worked and confirmed in `rows`: overall, by band and by mode class. */
  let dx = $derived.by(() => {
    const all = tally();
    const bands = new Map<string, Tally>();
    const classes = new Map<string, Tally>();
    for (const r of rows) {
      const ts = [all, bands.get(r.band) ?? tally(), classes.get(modeClass(r.mode)) ?? tally()];
      bands.set(r.band, ts[1]);
      classes.set(modeClass(r.mode), ts[2]);
      for (const t of ts) {
        t.qsos++;
        if (r.dxcc == null) continue;
        t.worked.add(r.dxcc);
        if (isConfirmed(r)) t.confirmed.add(r.dxcc);
      }
    }
    const order = view.contest.bands;
    return {
      all,
      bands: [...bands].sort(([a], [b]) => order.indexOf(a) - order.indexOf(b)),
      classes: (["PH", "CW", "DG"] as const).filter((c) => classes.has(c)).map((c) => [c, classes.get(c)!] as const),
    };
  });

  const CLASS_NAME = { PH: "Phone", CW: "CW", DG: "Data" };

  let rangeText = $derived(
    range.from && range.to
      ? `${range.from} to ${range.to}`
      : range.from
        ? `Since ${range.from}`
        : range.to
          ? `Up to ${range.to}`
          : "All dates",
  );

  let start = $derived(new Date(view.setup.start));
  let end = $derived(new Date(start.getTime() + view.contest.duration_minutes * 60_000));
  let usesMults = $derived(!!view.contest.multiplier);
  // Contests scored per band list every band; only show the ones in use.
  let groups = $derived.by(() => {
    const used = view.score.groups.filter((g) => g.qsos > 0);
    return view.score.groups.length > 3 ? used : view.score.groups;
  });
  let sessions = $derived(
    view.contest.sessions.map((s) => ({
      from: start.getTime() + s.start_minute * 60_000,
      to: start.getTime() + (s.start_minute + s.minutes) * 60_000,
    })),
  );

  function fmt(n: number) {
    return n % 1 ? n.toFixed(1) : n.toLocaleString("en-AU");
  }
</script>

<aside>
  {#if general}
    <section>
      <h3>DXCC countries</h3>
      <div class="total">{dx.all.worked.size}</div>
      <p>
        <span class="ok">{dx.all.confirmed.size} confirmed</span>
        <span class="muted">· {dx.all.qsos.toLocaleString()} QSOs</span>
      </p>
      <p class="muted">{rangeText}</p>
      {#if dx.classes.length}
        <table>
          <thead>
            <tr><th></th><th>QSOs</th><th>Worked</th><th>Cfmd</th></tr>
          </thead>
          <tbody>
            {#each dx.classes as [c, t] (c)}
              <tr><td>{CLASS_NAME[c]}</td><td>{t.qsos.toLocaleString()}</td><td>{t.worked.size}</td><td>{t.confirmed.size}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
    {#if dx.bands.length}
      <section>
        <h3>By band</h3>
        <table>
          <thead>
            <tr><th></th><th>QSOs</th><th>Worked</th><th>Cfmd</th></tr>
          </thead>
          <tbody>
            {#each dx.bands as [band, t] (band)}
              <tr><td>{band}</td><td>{t.qsos.toLocaleString()}</td><td>{t.worked.size}</td><td>{t.confirmed.size}</td></tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/if}
    <ClusterPanel {spots} status={clusterStatus} qsos={view.qsos} {onspot} />
  {:else}
    {#if view.contest.generic}
      <section>
        <h3>Contacts</h3>
        <div class="total">{view.score.qsos.toLocaleString()}</div>
        <p class="muted">Not scored here. Cabrillo lists this count as the claimed score.</p>
      </section>
    {:else}
    <section>
      <h3>Score</h3>
      <div class="total">{fmt(view.score.total)}</div>
      <table>
        <thead>
          <tr>
            <th></th>
            <th>QSOs</th>
            <th>Pts</th>
            {#if usesMults}<th>Mults</th>{/if}
            <th>Score</th>
          </tr>
        </thead>
        <tbody>
          {#each groups as g (g.name)}
            <tr>
              <td>{g.name}</td>
              <td>{g.qsos}</td>
              <td>{fmt(g.points)}</td>
              {#if usesMults}<td>{g.mults}</td>{/if}
              <td>{fmt(g.score)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if view.score.dupes || view.score.invalid}
        <p class="muted">{view.score.dupes} dupes, {view.score.invalid} invalid</p>
      {/if}
    </section>
    {/if}

    <section>
      <h3>Contest time</h3>
      <ContestTiming startIso={view.setup.start} contest={view.contest} />
      {#if sessions.length}
        {#each sessions as s, i (i)}
          <p class="muted">
            {i ? hhmm(new Date(s.from).toISOString()) : dateTimeText(new Date(s.from).toISOString())} – {hhmm(
              new Date(s.to).toISOString(),
            )}
            {zoneLabel()}
          </p>
        {/each}
      {:else}
        <p class="muted">{dateTimeText(start.toISOString())} – {hhmm(end.toISOString())} {zoneLabel()}</p>
      {/if}
    </section>

    {#if view.score.by_band.length}
      <section>
        <h3>By band</h3>
        <table>
          <tbody>
            {#each view.score.by_band as [band, n] (band)}
              <tr><td>{band}</td><td>{n}</td></tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/if}
  {/if}

  {#if check?.previous.length}
    <section>
      <h3>Worked before</h3>
      <table>
        <tbody>
          {#each general ? check.previous.slice(-20).reverse() : check.previous as p (p.id)}
            <tr class:dim={p.dupe}>
              <td class="mono">{general ? dateText(p.time) : hhmm(p.time)}</td>
              <td>{p.band}</td>
              <td>{p.mode}</td>
              {#if general}
                <td class="mono" class:ok={isConfirmed(p)} title={isConfirmed(p) ? "Confirmed" : ""}>
                  {isConfirmed(p) ? "✓" : ""}
                </td>
              {:else}
                <td class="mono">{[p.rcvd_nr, p.rcvd_exch, p.rcvd_locator].filter(Boolean).join(" ")}</td>
              {/if}
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
  }
  section {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 10px 12px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 0.8rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }
  p {
    margin: 2px 0;
  }
  .total {
    font-size: 2rem;
    font-weight: 700;
    color: var(--accent);
    margin-bottom: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.88rem;
  }
  th {
    text-align: right;
    font-weight: 500;
    color: var(--muted);
    font-size: 0.75rem;
  }
  td {
    padding: 2px 0;
  }
  td + td {
    text-align: right;
  }
  .muted {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .dim {
    opacity: 0.55;
  }
  .ok {
    color: var(--ok);
  }
</style>
