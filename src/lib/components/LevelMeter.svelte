<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { KEYER_LEVEL_EVENT } from "$lib/api";

  /** The microphone level while recording, 0 to 1. */
  let level = $state(0);

  onMount(() => {
    const unlisten = listen<number>(KEYER_LEVEL_EVENT, (e) => (level = e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  });

  // On a dB scale from -60 dBFS, so speech moves it well across the bar.
  let width = $derived(level > 0 ? Math.max(0, Math.min(100, ((20 * Math.log10(level) + 60) / 60) * 100)) : 0);
  let quiet = $derived(level < 0.003);
</script>

<span class="meter" title={quiet ? "No sound from the microphone" : "Microphone level"}>
  <span class="bar" class:hot={level > 0.9} style:width="{width}%"></span>
</span>
{#if quiet}<span class="quiet">no sound from the microphone</span>{/if}

<style>
  .meter {
    display: inline-block;
    vertical-align: middle;
    width: 8em;
    height: 0.6em;
    border: 1px solid var(--border);
    border-radius: 3px;
    overflow: hidden;
    background: var(--bg);
  }
  .bar {
    display: block;
    height: 100%;
    background: var(--ok);
    transition: width 0.08s linear;
  }
  .bar.hot {
    background: var(--bad);
  }
  .quiet {
    margin-left: 6px;
    font-size: 0.8rem;
    color: var(--warn);
  }
</style>
