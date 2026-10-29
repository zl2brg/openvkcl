<script lang="ts">
  import { hhmm, utcText, type CheckResult, type LogView } from "$lib/api";

  let {
    view,
    now,
    check,
  }: {
    view: LogView;
    now: Date;
    check: CheckResult | null;
  } = $props();

  let start = $derived(new Date(view.setup.start));
  let end = $derived(new Date(start.getTime() + view.contest.duration_minutes * 60_000));
  let usesMults = $derived(view.score.groups.some((g) => g.mults > 0) || !!view.contest.score_groups.length);

  let timing = $derived.by(() => {
    const t = now.getTime();
    if (t < start.getTime()) return `Starts in ${duration(start.getTime() - t)}`;
    if (t >= end.getTime()) return "Contest has ended";
    let text = `${duration(end.getTime() - t)} remaining`;
    const b = view.contest.blocks;
    if (b) {
      const block = Math.floor((t - start.getTime()) / (b.minutes * 60_000));
      const total = Math.ceil(view.contest.duration_minutes / b.minutes);
      text += ` · block ${block + 1} of ${total}`;
    }
    return text;
  });

  function duration(ms: number) {
    const mins = Math.floor(ms / 60_000);
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return h ? `${h}h ${m}m` : `${m}m`;
  }

  function fmt(n: number) {
    return n % 1 ? n.toFixed(1) : n.toLocaleString("en-AU");
  }
</script>

<aside>
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
        {#each view.score.groups as g (g.name)}
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

  <section>
    <h3>Contest time</h3>
    <p>{timing}</p>
    <p class="muted">{utcText(start.toISOString())} – {hhmm(end.toISOString())} UTC</p>
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

  {#if check?.previous.length}
    <section>
      <h3>Worked before</h3>
      <table>
        <tbody>
          {#each check.previous as p (p.id)}
            <tr class:dim={p.dupe}>
              <td class="mono">{hhmm(p.time)}</td>
              <td>{p.band}</td>
              <td>{p.mode}</td>
              <td class="mono">{p.rcvd_nr} {p.rcvd_locator}</td>
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
    font-size: 0.75rem;
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
</style>
