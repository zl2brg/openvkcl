<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    api,
    errorText,
    recordingName,
    KEYER_STATUS_EVENT,
    type KeyerConfig,
    type KeyerStatus,
    type KeySet,
  } from "$lib/api";

  let {
    config,
    phone,
    call,
    onchange,
  }: {
    config: KeyerConfig;
    /** Voice messages only play in phone modes. */
    phone: boolean;
    /** The call being typed: typing one stops CQ repeat, as in N1MM. */
    call: string;
    /** Save a change to the keyer settings (a new recording, the repeat time). */
    onchange: (config: KeyerConfig) => void;
  } = $props();

  let set = $state<KeySet>("run");
  let repeat = $state(false);
  let status = $state<KeyerStatus>({ playing: null, recording: null, finished: null, error: null });
  let note = $state("");
  let repeatInput: HTMLInputElement | undefined = $state();
  /** The next CQ when repeating, and when it's due. */
  let repeatTimer: ReturnType<typeof setTimeout> | undefined;
  let nextCq = $state<number | null>(null);

  let busy = $derived(!!status.playing || !!status.recording);
  let messages = $derived(set === "run" ? config.run : config.sp);
  let cqFile = $derived(config.run[0]?.file ?? "");

  onMount(() => {
    const unlisten = listen<KeyerStatus>(KEYER_STATUS_EVENT, (e) => {
      status = e.payload;
      if (e.payload.error) {
        note = e.payload.error;
        stopRepeat();
      }
      // The CQ ended by itself: send it again after the pause.
      if (repeat && e.payload.finished && e.payload.finished.endsWith(cqFile) && cqFile) {
        clearTimeout(repeatTimer);
        nextCq = Date.now() + config.repeat_ms;
        repeatTimer = setTimeout(() => {
          nextCq = null;
          if (repeat && !untrack(() => call).trim()) play("run", 0);
        }, config.repeat_ms);
      }
    });
    api.keyerInfo().then((i) => (status = i.status)).catch(() => {});
    return () => {
      clearTimeout(repeatTimer);
      unlisten.then((f) => f());
    };
  });

  function stopRepeat() {
    repeat = false;
    nextCq = null;
    clearTimeout(repeatTimer);
  }

  // Starting to type a callsign stops CQ repeat.
  $effect(() => {
    if (call.trim()) untrack(() => repeat && stopRepeat());
  });

  async function play(which: KeySet, key: number) {
    if (!phone) {
      note = "Voice messages play in phone modes (SSB, AM, FM)";
      return;
    }
    const msg = (which === "run" ? config.run : config.sp)[key];
    note = "";
    try {
      await api.keyerPlay(msg?.file ?? "");
    } catch (e) {
      note = errorText(e);
    }
  }

  async function record(key: number) {
    try {
      if (status.recording) {
        await api.keyerStop();
        return;
      }
      const file = recordingName(set, key);
      const list = (set === "run" ? config.run : config.sp).map((m, i) => (i === key ? { ...m, file } : m));
      onchange({ ...config, [set]: list });
      note = "";
      await api.keyerRecord(file);
    } catch (e) {
      note = errorText(e);
    }
  }

  async function stopAll() {
    stopRepeat();
    try {
      await api.keyerStop();
    } catch (e) {
      note = errorText(e);
    }
  }

  function setRepeatSeconds(text: string) {
    const s = Number(text);
    if (s > 0 && s <= 60) onchange({ ...config, repeat_ms: Math.round(s * 1000) });
  }

  /** Function keys work anywhere in the window except in a dialog. */
  function onkeydown(e: KeyboardEvent) {
    const fkey = /^F([1-9]|1[0-2])$/.exec(e.key);
    if (fkey) {
      // Keep the browser's own F-keys (F5 reload, F7 caret browsing) out of the way.
      e.preventDefault();
      if (document.querySelector("[role=dialog]")) return;
      const key = Number(fkey[1]) - 1;
      if ((e.ctrlKey || e.metaKey) && e.shiftKey) record(key);
      else if (e.shiftKey) play(set === "run" ? "sp" : "run", key);
      else if (key === 0) {
        // F1 is CQ: it always switches to Run, as in N1MM.
        set = "run";
        play("run", 0);
      } else play(set, key);
      return;
    }
    if (document.querySelector("[role=dialog]")) return;
    if (e.key === "Escape" && (busy || repeat)) {
      // Stop transmitting first; the entry line only clears on the next Esc.
      e.preventDefault();
      e.stopPropagation();
      stopAll();
    } else if (e.altKey && e.code === "KeyR") {
      e.preventDefault();
      if (repeat) stopRepeat();
      else repeat = true;
    } else if ((e.ctrlKey || e.metaKey) && e.code === "KeyR") {
      e.preventDefault();
      repeatInput?.focus();
      repeatInput?.select();
    } else if (e.altKey && e.code === "KeyU") {
      e.preventDefault();
      set = set === "run" ? "sp" : "run";
    }
  }

  function label(i: number) {
    return messages[i]?.label || "";
  }
</script>

<svelte:window onkeydowncapture={onkeydown} />

<div class="bar" class:off={!phone}>
  <div class="sets" role="radiogroup" aria-label="Operating mode" title="Run or Search & Pounce messages (Alt+U)">
    <button class="set" class:on={set === "run"} role="radio" aria-checked={set === "run"} onclick={() => (set = "run")} tabindex="-1"
      >Run</button
    >
    <button class="set" class:on={set === "sp"} role="radio" aria-checked={set === "sp"} onclick={() => (set = "sp")} tabindex="-1"
      >S&amp;P</button
    >
  </div>
  <div class="keys">
    {#each messages as m, i (i)}
      {@const file = set === "run" ? config.run[i]?.file : config.sp[i]?.file}
      <button
        class="key"
        class:playing={!!status.playing && !!file && status.playing.endsWith(file)}
        class:recording={!!status.recording && status.recording.endsWith(recordingName(set, i))}
        class:empty={!file}
        onclick={() => (i === 0 ? ((set = "run"), play("run", 0)) : play(set, i))}
        title="{file ? `Play ${file}` : 'Not recorded: Ctrl+Shift+F' + (i + 1) + ' records it'}"
        tabindex="-1"
      >
        <span class="f">F{i + 1}</span>
        <span class="label">{label(i)}</span>
      </button>
    {/each}
  </div>
  <button
    class="repeat"
    class:on={repeat}
    onclick={() => (repeat ? stopRepeat() : (repeat = true))}
    title="CQ repeat: F1 again after the pause, until you type a call or press Esc (Alt+R)"
    tabindex="-1">⟳ CQ</button
  >
  <label class="every" title="Pause between CQs (Ctrl+R)">
    <input
      bind:this={repeatInput}
      type="number"
      min="0.5"
      max="60"
      step="0.5"
      value={config.repeat_ms / 1000}
      onchange={(e) => setRepeatSeconds(e.currentTarget.value)}
      onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
    />s
  </label>
</div>
<div class="state">
  {#if status.recording}
    <span class="rec">● Recording {set === "run" ? "Run" : "S&P"} key: press Ctrl+Shift+F-key or Esc to stop</span>
  {:else if status.playing}
    <span class="tx">TX</span> <span>playing {status.playing.split(/[\\/]/).pop()} · Esc stops</span>
  {:else if repeat}
    <span class="muted">{nextCq ? "CQ again shortly" : "CQ repeat on: F1 to start"} · Esc or typing a call stops it</span>
  {:else if note}
    <span class="warn">{note}</span>
  {/if}
</div>

<style>
  .bar {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .bar.off .keys {
    opacity: 0.55;
  }
  .keys {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 4px;
  }
  button {
    font-size: 0.78rem;
    padding: 3px 4px;
  }
  .key {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    line-height: 1.15;
  }
  .key .f {
    font-size: 0.68rem;
    color: var(--muted);
    font-weight: 700;
  }
  .key .label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .key.empty .label {
    color: var(--muted);
  }
  .key.playing {
    background: var(--bad);
    border-color: var(--bad);
    color: var(--bg);
  }
  .key.playing .f {
    color: var(--bg);
  }
  .key.recording {
    border-color: var(--bad);
    box-shadow: inset 0 0 0 1px var(--bad);
  }
  .sets {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .set {
    flex: 1;
    min-width: 3.4em;
    font-weight: 700;
  }
  .set.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
  }
  .repeat.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
  }
  .every {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 0.78rem;
    color: var(--muted);
  }
  .every input {
    width: 3.6em;
    padding: 2px 4px;
    font-size: 0.78rem;
  }
  .state {
    min-height: 1.1rem;
    font-size: 0.82rem;
  }
  .tx {
    padding: 0 6px;
    border-radius: 3px;
    background: var(--bad);
    color: var(--bg);
    font-weight: 700;
  }
  .rec {
    color: var(--bad);
    font-weight: 600;
  }
  .warn {
    color: var(--warn);
  }
  .muted {
    color: var(--muted);
  }
</style>
