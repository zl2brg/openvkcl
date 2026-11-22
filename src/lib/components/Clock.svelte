<script lang="ts">
  import { onMount } from "svelte";
  import { clockText, zoneLabel } from "$lib/api";
  import { setLocalTime, timeZone } from "$lib/timezone.svelte";

  // Own timer so a once-a-second tick does not re-render the rest of the app.
  let now = $state(new Date());
  onMount(() => {
    const id = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(id);
  });
</script>

<button
  class="clock mono"
  onclick={() => setLocalTime(!timeZone.local)}
  title="Click to show times in {timeZone.local ? 'UTC' : 'local time'}. Logs and exports are always UTC."
>
  {clockText(now)} {zoneLabel()}
</button>

<style>
  .clock {
    font-size: 1.05rem;
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    cursor: pointer;
  }
</style>
