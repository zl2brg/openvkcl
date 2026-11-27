<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Modal from "./Modal.svelte";
  import { api, dateTimeText, errorText, zoneLabel, type ClusterConfig, type CodeListStatus, type CtyStatus, type QrzConfig, type ScpStatus } from "$lib/api";

  let {
    initial,
    initialCluster,
    stationCall,
    onsave,
    onctychange,
    oncodeschange,
    onclose,
  }: {
    initial: QrzConfig;
    initialCluster: ClusterConfig;
    /** Logs in to the cluster when no cluster callsign is set. */
    stationCall: string;
    onsave: (qrz: QrzConfig, cluster: ClusterConfig) => void;
    /** A new country file was loaded. */
    onctychange?: () => void;
    /** A new code list (e.g. Shire.db3) was loaded. */
    oncodeschange?: () => void;
    onclose: () => void;
  } = $props();

  let qrz = $state<QrzConfig>(untrack(() => $state.snapshot(initial)) as QrzConfig);
  let cluster = $state<ClusterConfig>(untrack(() => $state.snapshot(initialCluster)) as ClusterConfig);
  let scp = $state<ScpStatus | null>(null);
  let scpBusy = $state(false);
  let scpMessage = $state("");
  let scpError = $state(false);
  let cty = $state<CtyStatus | null>(null);
  let ctyBusy = $state(false);
  let ctyMessage = $state("");
  let ctyError = $state(false);
  let lists = $state<CodeListStatus[]>([]);
  let listBusy = $state("");
  let listMessage = $state<Record<string, { text: string; bad: boolean }>>({});
  let testBusy = $state(false);
  let testMessage = $state("");
  let testError = $state(false);

  onMount(async () => {
    try {
      scp = await api.scpStatus();
      cty = await api.ctyStatus();
      lists = await api.codeLists();
    } catch (e) {
      scpMessage = errorText(e);
      scpError = true;
    }
  });

  async function updateCty(action: () => Promise<CtyStatus | null>) {
    ctyBusy = true;
    ctyMessage = "";
    try {
      const s = await action();
      if (s) {
        cty = s;
        ctyMessage = `Loaded ${s.count} countries`;
        ctyError = false;
        onctychange?.();
      }
    } catch (e) {
      ctyMessage = errorText(e);
      ctyError = true;
    } finally {
      ctyBusy = false;
    }
  }

  const downloadCty = () => updateCty(api.ctyDownload);

  const importCty = () =>
    updateCty(async () => {
      const path = await open({
        title: "Import cty.csv",
        filters: [{ name: "Country file", extensions: ["csv"] }],
        multiple: false,
        directory: false,
      });
      return path ? api.ctyImport(path) : null;
    });

  async function updateList(file: string, action: () => Promise<CodeListStatus[] | null>) {
    listBusy = file;
    delete listMessage[file];
    try {
      const s = await action();
      if (s) {
        lists = s;
        const l = s.find((x) => x.file === file);
        listMessage[file] = { text: `Loaded ${l?.count ?? 0} ${l?.item ?? "code"}s`, bad: false };
        oncodeschange?.();
      }
    } catch (e) {
      listMessage[file] = { text: errorText(e), bad: true };
    } finally {
      listBusy = "";
    }
  }

  const downloadList = (file: string) => updateList(file, () => api.codeListDownload(file));

  const importList = (file: string) =>
    updateList(file, async () => {
      const ext = file.includes(".") ? file.split(".").pop()! : "";
      const path = await open({
        title: `Import ${file}`,
        filters: ext ? [{ name: file, extensions: [ext] }] : [],
        multiple: false,
        directory: false,
      });
      return path ? api.codeListImport(file, path) : null;
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

    <h3>Country file</h3>
    <p class="hint">
      In general logs, shows each station's DXCC country, continent and zones, and whether it's a new one for you.
    </p>
    <p class="status">
      {#if cty == null}
        …
      {:else if cty.count}
        <span class="ok">{cty.count} countries</span>
        {#if cty.updated}— updated {dateTimeText(cty.updated)} {zoneLabel()}{/if}
      {:else}
        No country file yet: countries won't be shown.
      {/if}
    </p>
    <div class="row">
      <button onclick={downloadCty} disabled={ctyBusy}>{ctyBusy ? "Working…" : "Download latest"}</button>
      <button onclick={importCty} disabled={ctyBusy}>Import file…</button>
    </div>
    {#if ctyMessage}<p class="status" class:bad={ctyError}>{ctyMessage}</p>{/if}
    <p class="hint">
      Downloads cty.csv from
      <a href="https://www.country-files.com" onclick={(e) => link(e, "https://www.country-files.com")}
        >country-files.com</a
      >, updated every few weeks as DXpeditions come and go.
    </p>

    {#each lists as l (l.file)}
      <h3>{l.contests.join(", ")} list</h3>
      <p class="hint">
        Checks the {l.item} codes received and sent in {l.contests.join(" and ")}, shows which {l.item} each code
        is, and lists matching {l.item}s as you type a code or place.
      </p>
      <p class="status">
        {#if l.count}
          <span class="ok">{l.count} {l.item}s</span>
          {#if l.updated}— updated {dateTimeText(l.updated)} {zoneLabel()}{/if}
        {:else if l.error}
          <span class="bad">{l.error}</span>
        {/if}
      </p>
      <div class="row">
        {#if l.url}
          <button onclick={() => downloadList(l.file)} disabled={!!listBusy}>
            {listBusy === l.file ? "Working…" : "Download latest"}
          </button>
        {/if}
        <button onclick={() => importList(l.file)} disabled={!!listBusy}>Import file…</button>
      </div>
      {#if listMessage[l.file]}
        <p class="status" class:bad={listMessage[l.file].bad}>{listMessage[l.file].text}</p>
      {/if}
      <p class="hint">
        {#if l.url}
          Downloads {l.file} from
          <a href={l.url} onclick={(e) => link(e, l.url)}>{new URL(l.url).hostname}</a>. Download it again when a new
          list is published, or import one you've been sent.
        {:else}
          Import {l.file} when a new list is published.
        {/if}
      </p>
    {/each}

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

    <h3>DX cluster</h3>
    <label class="check">
      <input type="checkbox" bind:checked={cluster.enabled} />
      Show spots of countries I still need, in general logs
    </label>
    <div class="row">
      <label class="host">Cluster <input bind:value={cluster.host} autocomplete="off" spellcheck="false" /></label>
      <label class="port">Port <input type="number" bind:value={cluster.port} min="1" max="65535" /></label>
      <label>
        Login callsign
        <input
          class="mono"
          bind:value={cluster.callsign}
          oninput={() => (cluster.callsign = cluster.callsign.toUpperCase())}
          placeholder={stationCall}
          autocomplete="off"
          spellcheck="false"
        />
      </label>
    </div>
    <p class="hint">
      Most clusters allow one login per callsign. If another program is already connected as {stationCall || "you"},
      add a number, e.g. {stationCall ? `${stationCall}-1` : "VK2ABC-1"}. The default is the WA9PIE network
      (hrd.wa9pie.net, port 8000).
    </p>
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button
      class="primary"
      onclick={() => onsave($state.snapshot(qrz) as QrzConfig, $state.snapshot(cluster) as ClusterConfig)}>Save</button
    >
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
  .row label.host {
    flex: 2;
  }
  .row label.port {
    flex: 0 0 6em;
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
