<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";
  import {
    api,
    errorText,
    recordingName,
    KEYER_STATUS_EVENT,
    type KeyerConfig,
    type KeyerInfo,
    type KeyerStatus,
    type KeySet,
  } from "$lib/api";

  let {
    initial,
    radioConnected,
    onsave,
    onclose,
  }: {
    initial: KeyerConfig;
    radioConnected: boolean;
    onsave: (config: KeyerConfig) => void;
    onclose: () => void;
  } = $props();

  let config = $state<KeyerConfig>(untrack(() => $state.snapshot(initial)) as KeyerConfig);
  let info = $state<KeyerInfo | null>(null);
  let status = $state<KeyerStatus>({ playing: null, recording: null, finished: null, error: null });
  let tab = $state<KeySet>("run");
  let message = $state("");

  onMount(() => {
    const unlisten = listen<KeyerStatus>(KEYER_STATUS_EVENT, (e) => {
      status = e.payload;
      if (e.payload.error) message = e.payload.error;
    });
    api
      .keyerInfo()
      .then((i) => {
        info = i;
        status = i.status;
      })
      .catch((e) => (message = errorText(e)));
    return () => {
      api.keyerStop().catch(() => {});
      unlisten.then((f) => f());
    };
  });

  let list = $derived(tab === "run" ? config.run : config.sp);
  let busy = $derived(!!status.playing || !!status.recording);

  /** Recording and testing use the settings as they are in this dialog. */
  async function withSaved(action: () => Promise<void>) {
    message = "";
    try {
      onsave($state.snapshot(config) as KeyerConfig);
      await action();
    } catch (e) {
      message = errorText(e);
    }
  }

  function record(i: number) {
    if (status.recording) return withSaved(() => api.keyerStop());
    const file = recordingName(tab, i);
    list[i].file = file;
    return withSaved(() => api.keyerRecord(file));
  }

  function test(i: number) {
    if (status.playing) return withSaved(() => api.keyerStop());
    return withSaved(() => api.keyerPlay(list[i].file));
  }

  async function choose(i: number) {
    const path = await open({
      title: `Message for ${tab === "run" ? "Run" : "S&P"} F${i + 1}`,
      filters: [{ name: "Sound", extensions: ["wav", "WAV"] }],
      multiple: false,
      directory: false,
    });
    if (path) list[i].file = path;
  }

  function fileName(f: string) {
    return f.split(/[\\/]/).pop() ?? f;
  }

  function isActive(i: number, what: string | null) {
    return !!what && !!list[i].file && what.endsWith(fileName(list[i].file));
  }
</script>

<Modal title="Voice keyer" {onclose} wide>
  <div class="form">
    <div class="row">
      <label>
        Sound output to the radio
        <select bind:value={config.output}>
          <option value="">System default</option>
          {#each info?.devices.outputs ?? [] as d (d)}<option value={d}>{d}</option>{/each}
          {#if config.output && info && !info.devices.outputs.includes(config.output)}
            <option value={config.output}>{config.output} (not connected)</option>
          {/if}
        </select>
      </label>
      <label>
        Microphone for recording
        <select bind:value={config.input}>
          <option value="">System default</option>
          {#each info?.devices.inputs ?? [] as d (d)}<option value={d}>{d}</option>{/each}
          {#if config.input && info && !info.devices.inputs.includes(config.input)}
            <option value={config.input}>{config.input} (not connected)</option>
          {/if}
        </select>
      </label>
    </div>
    <div class="row">
      <label class="check">
        <input type="checkbox" bind:checked={config.ptt} />
        Key the radio (PTT) through its connection
      </label>
      <label class="small">
        PTT delay
        <span><input type="number" min="0" max="1000" step="10" bind:value={config.ptt_delay_ms} /> ms</span>
      </label>
      <label class="small">
        CQ repeat pause
        <span
          ><input
            type="number"
            min="0.5"
            max="60"
            step="0.5"
            value={config.repeat_ms / 1000}
            onchange={(e) => (config.repeat_ms = Math.round(Number(e.currentTarget.value) * 1000) || config.repeat_ms)}
          /> s</span
        >
      </label>
    </div>
    <p class="hint">
      {#if config.ptt && !radioConnected}
        No radio is connected, so PTT can't be sent: the radio's VOX will have to key it.
      {:else if config.ptt}
        OpenVKCL keys the radio, waits the PTT delay, plays the message and unkeys. Esc stops at once.
      {:else}
        OpenVKCL only plays the message; the radio's VOX keys it.
      {/if}
      For a FlexRadio, choose its DAX transmit audio as the output.
    </p>

    <div class="tabs">
      <button class:on={tab === "run"} onclick={() => (tab = "run")}>Run</button>
      <button class:on={tab === "sp"} onclick={() => (tab = "sp")}>Search & Pounce</button>
    </div>
    <div class="keys">
      {#each list as m, i (i)}
        <div class="key">
          <span class="f">F{i + 1}</span>
          <input class="label" bind:value={m.label} placeholder="Label" maxlength="20" />
          <span class="file" title={m.file}>{m.file ? fileName(m.file) : "not recorded"}</span>
          <button
            class="rec"
            class:on={isActive(i, status.recording)}
            disabled={busy && !isActive(i, status.recording)}
            onclick={() => record(i)}
            title="Record from the microphone (Ctrl+Shift+F{i + 1} in the log window)"
            >{isActive(i, status.recording) ? "■ Stop" : "● Record"}</button
          >
          <button
            disabled={!m.file || (busy && !isActive(i, status.playing))}
            class:on={isActive(i, status.playing)}
            onclick={() => test(i)}
            title="Play it to the radio: this transmits">{isActive(i, status.playing) ? "■ Stop" : "▶ Play"}</button
          >
          <button onclick={() => choose(i)} disabled={busy} title="Use an existing .wav file">File…</button>
        </div>
      {/each}
    </div>
    <p class="hint">
      Recordings are saved in {info?.voice_dir ?? "the voice folder"}. F1 is the CQ key: it switches to Run, and
      CQ repeat sends it again after the pause. Shift+F-key plays the other set's key.
    </p>
    {#if message}<p class="error">{message}</p>{/if}
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button
      class="primary"
      onclick={() => {
        onsave($state.snapshot(config) as KeyerConfig);
        onclose();
      }}>Save</button
    >
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    gap: 12px;
    align-items: end;
    flex-wrap: wrap;
  }
  .row > label {
    flex: 1;
    min-width: 14em;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.85rem;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  label.small {
    flex: 0 0 auto;
    min-width: 0;
  }
  label.small input {
    width: 5em;
  }
  .hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs .on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg);
  }
  .keys {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .key {
    display: grid;
    grid-template-columns: 2.6em 10em 1fr auto auto auto;
    gap: 6px;
    align-items: center;
  }
  .key .f {
    font-weight: 700;
    color: var(--muted);
  }
  .key .file {
    font-size: 0.82rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .key button {
    font-size: 0.8rem;
    padding: 3px 8px;
  }
  .rec.on,
  button.on {
    background: var(--bad);
    border-color: var(--bad);
    color: var(--bg);
  }
</style>
