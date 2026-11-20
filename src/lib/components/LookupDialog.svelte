<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Modal from "./Modal.svelte";
  import { api, dateTimeText, errorText, zoneLabel, type QrzConfig, type ScpStatus } from "$lib/api";

  let {
    initial,
    onsave,
    onclose,
  }: {
    initial: QrzConfig;
    onsave: (config: QrzConfig) => void;
    onclose: () => void;
  } = $props();

  let qrz = $state<QrzConfig>(untrack(() => $state.snapshot(initial)) as QrzConfig);
  let scp = $state<ScpStatus | null>(null);
  let scpBusy = $state(false);
  let scpMessage = $state("");
  let scpError = $state(false);
  let testBusy = $state(false);
  let testMessage = $state("");
  let testError = $state(false);

  onMount(async () => {
    try {
      scp = await api.scpStatus();
    } catch (e) {
      scpMessage = errorText(e);
      scpError = true;
    }
  });

  async function updateScp(action: () => Promise<ScpStatus | null>) {
    scpBusy = true;
    scpMessage = "";
    try {
      const s = await action();
      if (s) {
        scp = s;
        scpMessage = `Loaded ${s.count.toLocaleString()} calls`;
        scpError = false;
      }
    } catch (e) {
      scpMessage = errorText(e);
      scpError = true;
    } finally {
      scpBusy = false;
    }
  }

  const download = () => updateScp(api.scpDownload);

  const importFile = () =>
    updateScp(async () => {
      const path = await open({
        title: "Import MASTER.SCP",
        filters: [{ name: "Super Check Partial", extensions: ["scp", "SCP", "txt"] }],
        multiple: false,
        directory: false,
      });
      return path ? api.scpImport(path) : null;
    });

  async function test() {
    testBusy = true;
    testMessage = "";
    try {
      testMessage = await api.qrzTest($state.snapshot(qrz) as QrzConfig);
      testError = false;
    } catch (e) {
      testMessage = errorText(e);
      testError = true;
    } finally {
      testBusy = false;
    }
  }

  function link(e: MouseEvent, url: string) {
    e.preventDefault();
    openUrl(url);
  }
</script>

<Modal title="Callsign lookups" {onclose}>
  <div class="form">
    <h3>Super Check Partial</h3>
    <p class="hint">
      As you type a callsign, matching calls from MASTER.SCP and your log are shown under the entry line, with calls
      one character different (N+1) after them. Type <code>?</code> for any one character.
    </p>
    <p class="status">
      {#if scp == null}
        …
      {:else if scp.count}
        <span class="ok">{scp.count.toLocaleString()} calls</span>
        {#if scp.updated}— updated {dateTimeText(scp.updated)} {zoneLabel()}{/if}
      {:else}
        No MASTER.SCP yet: only calls in your log will be suggested.
      {/if}
    </p>
    <div class="row">
      <button onclick={download} disabled={scpBusy}>{scpBusy ? "Working…" : "Download latest"}</button>
      <button onclick={importFile} disabled={scpBusy}>Import file…</button>
    </div>
    {#if scpMessage}<p class="status" class:bad={scpError}>{scpMessage}</p>{/if}
    <p class="hint">
      Downloads from
      <a href="https://www.supercheckpartial.com" onclick={(e) => link(e, "https://www.supercheckpartial.com")}
        >supercheckpartial.com</a
      >. Update it before each contest.
    </p>

    <h3>QRZ.com</h3>
    <label class="check">
      <input type="checkbox" bind:checked={qrz.enabled} />
      Look up each callsign on QRZ.com
    </label>
    <div class="row">
      <label>Username <input bind:value={qrz.username} autocomplete="off" spellcheck="false" /></label>
      <label>Password <input type="password" bind:value={qrz.password} autocomplete="off" /></label>
    </div>
    <div class="row">
      <button onclick={test} disabled={testBusy || !qrz.username || !qrz.password}>
        {testBusy ? "Checking…" : "Test login"}
      </button>
    </div>
    {#if testMessage}<p class="status" class:bad={testError} class:ok={!testError}>{testMessage}</p>{/if}
    <p class="hint">
      Name, location and grid need a QRZ.com XML Logbook Data subscription; free accounts get name and country.
      Your password is stored in OpenVKCL's settings file on this computer.
    </p>
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={() => onsave($state.snapshot(qrz) as QrzConfig)}>Save</button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    margin: 6px 0 0;
    font-size: 0.95rem;
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
    display: flex;
    gap: 8px;
    align-items: end;
  }
  .row label {
    flex: 1;
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
