<script lang="ts">
  import { codeMatches, type ContestDef } from "$lib/api";

  /**
   * A drop-down of listed codes (e.g. VK shires) matching what's typed in the
   * field it sits under, by code or by name. The field passes its keys to key().
   */
  let {
    contest,
    typed,
    open,
    onpick,
  }: {
    contest: ContestDef;
    typed: string;
    /** Shown only while the field has focus. */
    open: boolean;
    onpick: (code: string) => void;
  } = $props();

  let matches = $derived(codeMatches(contest, typed));
  let active = $state(0);
  let dismissed = $state(false);
  let shown = $derived(open && !dismissed && matches.length > 0);

  // A new search starts at the top and reopens the list.
  $effect(() => {
    typed;
    active = 0;
    dismissed = false;
  });

  /**
   * Handles a key in the field: arrows move through the list, Enter, Space or
   * Tab takes the highlighted code, Esc closes the list. Returns true when the
   * key was used, so Enter doesn't also log the contact. Space and Tab still
   * move on to the next field after taking the code.
   */
  export function key(e: KeyboardEvent): boolean {
    if (!shown) return false;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      active = (active + (e.key === "ArrowDown" ? 1 : -1) + matches.length) % matches.length;
    } else if (e.key === "Enter") {
      onpick(matches[active][0]);
    } else if (e.key === " " || (e.key === "Tab" && !e.shiftKey)) {
      onpick(matches[active][0]);
      e.preventDefault();
      return false;
    } else if (e.key === "Escape") {
      dismissed = true;
    } else {
      return false;
    }
    e.preventDefault();
    e.stopPropagation();
    return true;
  }
</script>

{#if shown}
  <ul class="suggest" role="listbox">
    {#each matches as [code, name], i (code)}
      <li role="option" aria-selected={i === active}>
        <!-- mousedown, so the field keeps focus -->
        <button
          type="button"
          tabindex="-1"
          class:active={i === active}
          onmousedown={(e) => {
            e.preventDefault();
            onpick(code);
          }}
        >
          <span class="mono code">{code}</span>
          <span class="name">{name}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .suggest {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    min-width: 18rem;
    margin: 2px 0 0;
    padding: 2px;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    box-shadow: 0 4px 12px rgb(0 0 0 / 0.25);
  }
  button {
    display: flex;
    gap: 0.75rem;
    width: 100%;
    padding: 2px 6px;
    border: 0;
    border-radius: 3px;
    background: none;
    color: inherit;
    text-align: left;
    font-size: 0.9rem;
    font-weight: normal;
    cursor: pointer;
  }
  button:hover {
    background: var(--hover);
  }
  button.active {
    background: var(--highlight);
  }
  .code {
    min-width: 3.5ch;
    font-weight: 600;
  }
  .name {
    white-space: nowrap;
  }
</style>
