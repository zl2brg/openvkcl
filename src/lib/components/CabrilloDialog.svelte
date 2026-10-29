<script lang="ts">
  import Modal from "./Modal.svelte";

  let {
    text,
    onsave,
    onclose,
  }: {
    text: string;
    onsave: () => void;
    onclose: () => void;
  } = $props();
</script>

<Modal title="Cabrillo 3.0 log" {onclose} wide>
  <p class="hint">Check the header before saving. Upload the saved file at vklogchecker.com.</p>
  <pre>{text}</pre>

  {#snippet footer()}
    <button onclick={onclose}>Close</button>
    <button class="primary" onclick={onsave}>Save as…</button>
  {/snippet}
</Modal>

<style>
  pre {
    margin: 0;
    padding: 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.8rem;
    max-height: 60vh;
    overflow: auto;
  }
  .hint {
    margin: 0 0 8px;
    font-size: 0.85rem;
    color: var(--muted);
  }
</style>
