<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { formatFreq, type RigConfig, type RigKind, type RigStatus } from "$lib/api";

  let {
    initial,
    status,
    onconnect,
    onclose,
  }: {
    initial: RigConfig;
    status: RigStatus;
    onconnect: (config: RigConfig) => void;
    onclose: () => void;
  } = $props();

  let config = $state<RigConfig>(untrack(() => $state.snapshot(initial)) as RigConfig);

  const defaultPorts: Record<RigKind, number> = { none: 0, rigctld: 4532, flrig: 12345, tci: 50001 };

  function kindChanged() {
    if (config.kind !== "none") config.port = defaultPorts[config.kind];
  }
</script>

<Modal title="Radio control" {onclose}>
  <div class="form">
    <label>
      Connect via
      <select bind:value={config.kind} onchange={kindChanged}>
        <option value="none">No radio (manual band and mode)</option>
        <option value="rigctld">Hamlib rigctld</option>
        <option value="flrig">FLRig</option>
        <option value="tci">TCI (cudaSDR, ExpertSDR, Thetis)</option>
      </select>
    </label>
    {#if config.kind !== "none"}
      <div class="row">
        <label>Host <input bind:value={config.host} /></label>
        <label>Port <input type="number" bind:value={config.port} min="1" max="65535" /></label>
      </div>
      {#if config.kind === "tci"}
        <label class="check">
          <input type="checkbox" bind:checked={config.spots} />
          Mark logged contacts on the SDR's band display
        </label>
      {:else}
        <label>
          Poll interval (ms)
          <input type="number" bind:value={config.poll_ms} min="100" max="5000" step="100" />
        </label>
      {/if}
      {#if config.kind === "tci"}
        <p class="hint">
          Enable the TCI server in your SDR software. cudaSDR listens on port 50001, ExpertSDR on 40001.
          The SDR pushes frequency and mode changes, so the entry line follows the radio instantly.
        </p>
      {:else if config.kind === "rigctld"}
        <p class="hint">
          Start Hamlib's network daemon for your radio first, e.g.
          <code>rigctld -m 3073 -r /dev/ttyUSB0 -s 19200</code> (on Windows use the COM port, e.g. <code>-r COM3</code>).
          Run <code>rigctl -l</code> to find your radio's model number.
        </p>
      {:else}
        <p class="hint">Start FLRig and configure your radio in it; OpenVKCL reads the frequency and mode over XML-RPC.</p>
      {/if}
    {/if}
    <p class="status">
      Status:
      {#if status.connected}
        <span class="ok">connected</span> — {formatFreq(status.freq_hz)} kHz {status.rig_mode ?? ""}
      {:else if status.error}
        <span class="bad">{status.error}</span>
      {:else}
        not connected
      {/if}
    </p>
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Close</button>
    <button class="primary" onclick={() => onconnect($state.snapshot(config) as RigConfig)}>
      {config.kind === "none" ? "Disconnect" : "Connect"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
  .row {
    display: grid;
    grid-template-columns: 2fr 1fr;
    gap: 8px;
  }
  .hint {
    font-size: 0.8rem;
    color: var(--muted);
    margin: 0;
  }
  .status {
    margin: 0;
    font-size: 0.85rem;
  }
  .ok {
    color: var(--ok);
  }
  .bad {
    color: var(--bad);
  }
</style>
