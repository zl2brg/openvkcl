<script lang="ts">
  import { onMount } from "svelte";
  import { countdownText, untilText } from "$lib/api";

  let {
    at,
    mode = "countdown",
    title = "",
  }: {
    /** ISO time the countdown ends. */
    at: string;
    /** "countdown" → 2:59:41; "until" → in 60 mins. */
    mode?: "countdown" | "until";
    title?: string;
  } = $props();

  let now = $state(new Date());
  let text = $derived(mode === "until" ? untilText(at, now) : countdownText(at, now));

  onMount(() => {
    // Minute-level "until" text only needs a slow tick; second countdowns need 1 Hz.
    const ms = mode === "until" ? 15_000 : 1000;
    const id = setInterval(() => (now = new Date()), ms);
    return () => clearInterval(id);
  });
</script>

{#if text}
  {#if mode === "countdown"}
    <span class="rework" {title}>⏱ {text}</span>
  {:else}
    <span {title}>can work again {text}</span>
  {/if}
{/if}

<style>
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
    white-space: nowrap;
  }
</style>
