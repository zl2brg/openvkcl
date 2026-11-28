<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorText, type ShareFound, type ShareOffer } from "$lib/api";
  import Modal from "./Modal.svelte";

  let {
    client = 0,
    defaultOperator,
    queued,
    queueHost,
    onjoin,
    ondiscard,
    onclose,
  }: {
    /** This copy's number. Its own band stays available when it joins again. */
    client?: number;
    defaultOperator: string;
    queued: number;
    queueHost: string;
    onjoin: (addr: string, operator: string, band: string) => Promise<void>;
    ondiscard: () => Promise<void>;
    onclose: () => void;
  } = $props();

  let addr = $state("");
  let operator = $state("");
  onMount(() => {
    operator = defaultOperator;
  });
  let scoreboard = $state(false);
  let band = $state("");
  let offer = $state<ShareOffer | null>(null);
  let found = $state<ShareFound[]>([]);
  let error = $state("");
  let busy = $state(false);

  let taken = $derived.by(() => {
    const held = new Set(
      (offer?.stations ?? []).filter((s) => s.band && !(client && s.client === client)).map((s) => s.band),
    );
    return held;
  });

  async function findHosts() {
    busy = true;
    error = "";
    try {
      found = await api.multiDiscover();
      if (!found.length) error = "No shared logs heard. Type the host address.";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function lookUp() {
    if (!addr.trim()) {
      error = "Enter the host address";
      return;
    }
    busy = true;
    error = "";
    try {
      offer = await api.multiPeek(addr.trim());
      const held = new Set(offer.stations.filter((s) => s.band && !(client && s.client === client)).map((s) => s.band));
      const free = offer.bands.find((b) => !held.has(b));
      if (!band || !offer.bands.includes(band) || held.has(band)) band = free ?? "";
    } catch (e) {
      offer = null;
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function choose(host: ShareFound) {
    addr = host.addr;
    await lookUp();
  }

  async function join() {
    busy = true;
    error = "";
    try {
      await onjoin(addr.trim(), operator.trim(), scoreboard ? "" : band);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function discard() {
    busy = true;
    error = "";
    try {
      await ondiscard();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Join shared log" {onclose}>
  <form
    class="join"
    onsubmit={(e) => {
      e.preventDefault();
      join();
    }}
  >
    <label>
      Host
      <span class="host">
        <input bind:value={addr} placeholder="192.168.1.20 or 192.168.1.20:7374" spellcheck="false" />
        <button type="button" onclick={findHosts} disabled={busy}>Find</button>
      </span>
    </label>
    {#if found.length}
      <ul>
        {#each found as host (host.addr)}
          <li>
            <button type="button" onclick={() => choose(host)}>
              <span class="mono">{host.call}</span>
              {host.contest}
              <span class="muted">{host.addr}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <label>
      Operator
      <input class="mono" bind:value={operator} spellcheck="false" />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={scoreboard} />
      Scoreboard only
    </label>
    {#if offer}
      <p class="muted">{offer.contest} · {offer.call}</p>
      {#if !scoreboard}
        <label>
          Band
          <select bind:value={band}>
            {#each offer.bands as b (b)}
              <option value={b} disabled={taken.has(b)}>{b}{taken.has(b) ? " (in use)" : ""}</option>
            {/each}
          </select>
        </label>
      {/if}
    {:else}
      <button type="button" onclick={lookUp} disabled={busy}>Check address</button>
    {/if}
    {#if queued > 0}
      <p>
        {queued} contact{queued === 1 ? "" : "s"} held for {queueHost || "the last host"}.
        <button type="button" onclick={discard} disabled={busy}>Discard</button>
      </p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    <button class="primary" onclick={join} disabled={busy || !offer || (!scoreboard && !band)}>Join</button>
  {/snippet}
</Modal>

<style>
  .join {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  .host {
    display: flex;
    gap: 8px;
  }
  .host input {
    flex: 1;
  }
  input,
  select,
  button {
    font: inherit;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  ul button {
    width: 100%;
    text-align: left;
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  .muted {
    color: var(--muted);
  }
  .error {
    color: var(--bad);
    margin: 0;
  }
  .mono {
    font-family: var(--mono, ui-monospace, monospace);
  }
</style>
