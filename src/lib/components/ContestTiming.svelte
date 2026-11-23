<script lang="ts">
  import { onMount } from "svelte";
  import type { ContestDef } from "$lib/api";

  let {
    startIso,
    contest,
  }: {
    startIso: string;
    contest: ContestDef;
  } = $props();

  let now = $state(new Date());
  let start = $derived(new Date(startIso));
  let end = $derived(new Date(start.getTime() + contest.duration_minutes * 60_000));
  let sessions = $derived(
    contest.sessions.map((s) => ({
      from: start.getTime() + s.start_minute * 60_000,
      to: start.getTime() + (s.start_minute + s.minutes) * 60_000,
    })),
  );

  let timing = $derived.by(() => {
    const t = now.getTime();
    if (t < start.getTime()) return `Starts in ${duration(start.getTime() - t)}`;
    if (t >= end.getTime()) return "Contest has ended";
    if (sessions.length) {
      const i = sessions.findIndex((s) => t < s.to);
      if (i < 0) return "Contest has ended";
      const s = sessions[i];
      if (t < s.from) return `Break · period ${i + 1} of ${sessions.length} starts in ${duration(s.from - t)}`;
      return `Period ${i + 1} of ${sessions.length} · ${duration(s.to - t)} remaining · hour ${Math.floor((t - s.from) / 3_600_000) + 1}`;
    }
    let text = `${duration(end.getTime() - t)} remaining`;
    const b = contest.blocks;
    if (b) {
      const block = Math.floor((t - start.getTime()) / (b.minutes * 60_000));
      const total = Math.ceil(contest.duration_minutes / b.minutes);
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

  onMount(() => {
    // Text is minute-granularity; a slow tick is enough and avoids panel flicker.
    const id = setInterval(() => (now = new Date()), 15_000);
    return () => clearInterval(id);
  });
</script>

<p>{timing}</p>
