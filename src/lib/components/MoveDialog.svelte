<script lang="ts">
  import Modal from "./Modal.svelte";
  import CodeSuggest from "./CodeSuggest.svelte";
  import { codeMatches, numberMeaning, type ContestDef } from "$lib/api";

  /** A rover has moved: choose the code (e.g. shire) it sends from now on. */
  let {
    contest,
    current,
    onsave,
    onclose,
  }: {
    contest: ContestDef;
    current: string;
    onsave: (code: string) => void;
    onclose: () => void;
  } = $props();

  let code = $state("");
  let focused = $state(false);
  let suggest = $state<CodeSuggest>();
  let error = $state("");
  let input = $state<HTMLInputElement>();
  // The field starts with the cursor in it (autofocus doesn't reach a dialog opened later).
  $effect(() => input?.focus());
  let list = $derived(contest.number_list);
  let item = $derived(list?.item ?? contest.number_label.toLowerCase());
  let meaning = $derived(code.trim() ? numberMeaning(contest, code) : null);

  function save() {
    const c = code.trim().toUpperCase();
    if (!/^[A-Z0-9]+$/.test(c)) {
      error = `Enter the ${item} you're in now.`;
      return;
    }
    if (meaning === "") {
      error = `${c} is not a listed ${item}.`;
      return;
    }
    onsave(c);
  }
</script>

<Modal title="Change {item}" {onclose}>
  <p class="hint">Now sending {current}.</p>
  <label>
    {item[0].toUpperCase() + item.slice(1)} you're in now
    <input
      bind:this={input}
      class="mono"
      bind:value={code}
      oninput={() => (code = code.toUpperCase())}
      onkeydown={(e) => {
        if (suggest?.key(e)) return;
        if (e.key === "Enter") save();
      }}
      onfocus={() => (focused = true)}
      onblur={() => (focused = false)}
      spellcheck="false"
      autocomplete="off"
      placeholder="Code or place"
    />
    <CodeSuggest bind:this={suggest} {contest} typed={code} open={focused} onpick={(c) => (code = c)} />
  </label>
  {#if meaning}
    <p class="hint">{meaning}</p>
  {:else if meaning === "" && !codeMatches(contest, code).length}
    <p class="hint warn">Not a listed {item}.</p>
  {/if}
  {#if contest.rover?.move_minutes}
    <p class="hint">Wait {contest.rover.move_minutes} minutes after your last contact before the next one.</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={save}>Change</button>
  {/snippet}
</Modal>

<style>
  label {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.9rem;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 8px;
    font-size: 0.85rem;
    color: var(--muted);
  }
  label + .hint {
    margin-top: 6px;
  }
  .hint.warn {
    color: var(--warn);
  }
</style>
