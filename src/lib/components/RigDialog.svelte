<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import {
    api,
    errorText,
    formatFreq,
    type RigConfig,
    type RigKind,
    type RigModel,
    type RigStatus,
    type SerialPort,
  } from "$lib/api";

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

  const defaultPorts: Record<RigKind, number> = { none: 0, hamlib: 0, rigctld: 4532, flrig: 12345, tci: 50001 };
  const bauds = [4800, 9600, 19200, 38400, 57600, 115200];

  let models = $state<RigModel[]>([]);
  let modelsError = $state("");
  let ports = $state<SerialPort[]>([]);
  let search = $state("");
  let otherPort = $state(false);

  let modelName = $derived.by(() => {
    const m = models.find((x) => x.id === config.model);
    return m ? `${m.mfg} ${m.model}` : "";
  });
  let shownModels = $derived.by(() => {
    const words = search.toLowerCase().split(/\s+/).filter(Boolean);
    return models.filter((m) => {
      const text = `${m.mfg} ${m.model} ${m.id}`.toLowerCase();
      return words.every((w) => text.includes(w));
    });
  });
  let portMissing = $derived(!!config.serial_port && !ports.some((p) => p.path === config.serial_port));

  async function loadHamlib() {
    modelsError = "";
    try {
      [models, ports] = await Promise.all([models.length ? models : api.rigModels(), api.serialPorts()]);
    } catch (e) {
      modelsError = errorText(e);
    }
  }

  async function refreshPorts() {
    ports = await api.serialPorts();
  }

  $effect(() => {
    if (config.kind === "hamlib") untrack(loadHamlib);
  });

  function kindChanged() {
    if (defaultPorts[config.kind]) config.port = defaultPorts[config.kind];
  }

  function portChanged(e: Event) {
    const value = (e.currentTarget as HTMLSelectElement).value;
    otherPort = value === "";
    if (!otherPort) config.serial_port = value;
  }
</script>

<Modal title="Radio control" {onclose}>
  <div class="form">
    <label>
      Connect via
      <select bind:value={config.kind} onchange={kindChanged}>
        <option value="none">No radio (manual band and mode)</option>
        <option value="hamlib">Radio on a USB or serial port (Hamlib)</option>
        <option value="rigctld">Hamlib rigctld already running</option>
        <option value="flrig">FLRig</option>
        <option value="tci">TCI (cudaSDR, ExpertSDR, Thetis)</option>
      </select>
    </label>
    {#if config.kind === "hamlib"}
      {#if modelsError}
        <p class="bad">{modelsError}</p>
      {:else}
        <label>
          Radio model {#if modelName}<span class="muted">— {modelName}</span>{/if}
          <input bind:value={search} placeholder="Search, e.g. 9700 or Yaesu" />
          <select class="models" size="7" bind:value={config.model}>
            {#each shownModels as m (m.id)}
              <option value={m.id}>{m.mfg} {m.model}</option>
            {/each}
          </select>
        </label>
        <div class="row">
          <label>
            Serial port
            <select value={otherPort ? "" : config.serial_port} onchange={portChanged}>
              {#if !config.serial_port && !otherPort}
                <option value={config.serial_port} disabled>Choose…</option>
              {/if}
              {#each ports as p (p.path)}
                <option value={p.path}>{p.label}</option>
              {/each}
              {#if portMissing && !otherPort}
                <option value={config.serial_port}>{config.serial_port} (not found)</option>
              {/if}
              <option value="">Other…</option>
            </select>
          </label>
          <button class="refresh" onclick={refreshPorts} title="Look for serial ports again">Refresh</button>
        </div>
        {#if otherPort}
          <label>
            Port name
            <input class="mono" bind:value={config.serial_port} placeholder="/dev/ttyUSB0 or COM3" />
          </label>
        {/if}
        <div class="row">
          <label>
            Baud rate
            <select bind:value={config.baud}>
              <option value={0}>Radio's default</option>
              {#each bauds as b (b)}
                <option value={b}>{b}</option>
              {/each}
            </select>
          </label>
          <label>
            Poll interval (ms)
            <input type="number" bind:value={config.poll_ms} min="100" max="5000" step="100" />
          </label>
        </div>
        <label>
          Extra rigctld options
          <input class="mono" bind:value={config.rigctld_args} placeholder="optional, e.g. --set-conf=civaddr=0xA2" />
        </label>
        <p class="hint">
          OpenVKCL runs Hamlib's rigctld for you while connected. If the radio doesn't answer, set the baud rate to
          match the radio's CAT or CI-V USB setting.
        </p>
      {/if}
    {:else if config.kind !== "none"}
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
          For a rigctld you start yourself, e.g. on another computer:
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
    align-items: end;
  }
  .models {
    font-size: 0.85rem;
  }
  .refresh {
    justify-self: start;
  }
  .hint {
    font-size: 0.8rem;
    color: var(--muted);
    margin: 0;
  }
  .muted {
    color: var(--muted);
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
