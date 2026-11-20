<script lang="ts">
  import "../app.css";
  // Apply the saved appearance before the first paint of the main page.
  import "$lib/theme.svelte";

  let { children } = $props();
</script>

{@render children()}
