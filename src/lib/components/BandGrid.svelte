<script lang="ts">
  import { isConfirmed, modeClass, type DxCheck, type Need, type QsoRow } from "$lib/api";

  let {
    bands,
    band,
    mode,
    call,
    previous,
    dx,
  }: {
    /** Every band the log offers, in order. */
    bands: string[];
    /** The band and mode on the entry line, outlined. */
    band: string;
    mode: string;
    call: string;
    /** Earlier contacts with this station. */
    previous: QsoRow[];
    dx: DxCheck | null;
  } = $props();

  // The HF bands and 6m always show; others only once worked or in use.
  const ALWAYS = ["160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m"];
  const CLASSES = [
    ["PH", "Phone"],
    ["CW", "CW"],
    ["DG", "Data"],
  ] as const;

  const better = (a: Need | undefined, b: Need): Need => (a === "confirmed" || b === "confirmed" ? "confirmed" : b);

  /** This station on each band and mode class, from the earlier contacts. */
  let station = $derived.by(() => {
    const m = new Map<string, Need>();
    for (const q of previous) {
      const k = `${q.band}|${modeClass(q.mode)}`;
      m.set(k, better(m.get(k), isConfirmed(q) ? "confirmed" : "worked"));
    }
    return m;
  });

  let country = $derived(new Map((dx?.slots ?? []).map((s) => [`${s.band}|${s.class}`, s.need])));

  let columns = $derived.by(() => {
    const used = new Set([...ALWAYS, band, ...previous.map((q) => q.band), ...(dx?.slots ?? []).map((s) => s.band)]);
    return bands.filter((b) => used.has(b));
  });

  let current = $derived(modeClass(mode));

  function cell(b: string, c: string) {
    const st = station.get(`${b}|${c}`);
    const ct = country.get(`${b}|${c}`);
    const parts = [`${b} ${CLASSES.find((x) => x[0] === c)![1]}`];
    if (call) parts.push(st ? `${call} ${st}` : `${call} not worked`);
    if (dx) parts.push(ct ? `${dx.country.country} ${ct}` : `${dx.country.country} not worked`);
    return { st, ct, title: parts.join(" · ") };
  }
</script>

<div class="grid" aria-label="Bands and modes worked">
  <table>
    <thead>
      <tr>
        <th></th>
        {#each columns as b (b)}
          <th class:here={b === band}>{b.replace(/m$/, "")}</th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each CLASSES as [c, label] (c)}
        <tr>
          <th class="row" class:here={c === current}>{label}</th>
          {#each columns as b (b)}
            {@const x = cell(b, c)}
            <td
              class:here={b === band && c === current}
              class:st-w={x.st === "worked"}
              class:st-c={x.st === "confirmed"}
              class:ct-w={!x.st && x.ct === "worked"}
              class:ct-c={!x.st && x.ct === "confirmed"}
              title={x.title}
            >
              {x.st === "confirmed" ? "C" : x.st === "worked" ? "W" : ""}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
  <div class="legend">
    <span><i class="st-w">W</i> worked</span>
    <span><i class="st-c">C</i> confirmed</span>
    <span><i class="ct-w"></i><i class="ct-c"></i> country</span>
  </div>
</div>

<style>
  .grid {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  table {
    border-collapse: separate;
    border-spacing: 2px;
    font-size: 0.7rem;
  }
  th {
    font-weight: 600;
    color: var(--muted);
    text-align: center;
    padding: 0 1px;
  }
  th.row {
    text-align: right;
    padding-right: 4px;
  }
  th.here {
    color: var(--accent);
  }
  td {
    width: 22px;
    height: 14px;
    text-align: center;
    font-weight: 700;
    line-height: 14px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  td.here {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .st-w {
    background: var(--warn);
    border-color: var(--warn);
    color: var(--bg);
  }
  .st-c {
    background: var(--ok);
    border-color: var(--ok);
    color: var(--bg);
  }
  .ct-w {
    background: color-mix(in srgb, var(--warn) 25%, transparent);
  }
  .ct-c {
    background: color-mix(in srgb, var(--ok) 25%, transparent);
  }
  .legend {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    font-size: 0.7rem;
    color: var(--muted);
  }
  .legend span {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .legend i {
    display: inline-block;
    width: 14px;
    height: 11px;
    border: 1px solid var(--border);
    border-radius: 2px;
    font-style: normal;
    font-weight: 700;
    font-size: 0.6rem;
    line-height: 11px;
    text-align: center;
  }
</style>
