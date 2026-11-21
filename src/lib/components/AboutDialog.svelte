<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Modal from "./Modal.svelte";

  let { version, onclose }: { version: string; onclose: () => void } = $props();

  const email = "simon.eatough@gmail.com";
  const github = "https://github.com/zl2brg/openvkcl";
  const gpl = "https://www.gnu.org/licenses/gpl-3.0.html";
</script>

<Modal title="About OpenVKCL" {onclose}>
  <div class="about">
    <h3>OpenVKCL</h3>
    <p class="muted">Version {version}</p>
    <p>A contest logger for Australian and New Zealand contests.</p>
    <dl>
      <dt>Author</dt>
      <dd>Simon, ZL2BRG</dd>
      <dt>Email</dt>
      <dd><a href="mailto:{email}" onclick={(e) => (e.preventDefault(), openUrl(`mailto:${email}`))}>{email}</a></dd>
      <dt>Project</dt>
      <dd><a href={github} onclick={(e) => (e.preventDefault(), openUrl(github))}>{github}</a></dd>
      <dt>Licence</dt>
      <dd><a href={gpl} onclick={(e) => (e.preventDefault(), openUrl(gpl))}>GNU GPL version 3 or later</a></dd>
    </dl>
    <p class="muted small">
      Copyright © 2026 Simon, ZL2BRG. OpenVKCL is free software: you can redistribute and modify it under the terms
      of the GNU General Public License. It comes with ABSOLUTELY NO WARRANTY.
    </p>
    <p class="muted small">Radio control uses <a href="https://hamlib.github.io" onclick={(e) => (e.preventDefault(), openUrl("https://hamlib.github.io"))}>Hamlib</a>.</p>
  </div>

  {#snippet footer()}
    <button class="primary" onclick={onclose}>Close</button>
  {/snippet}
</Modal>

<style>
  .about {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    margin: 0;
    font-size: 1.3rem;
  }
  p {
    margin: 0;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 16px;
    margin: 10px 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  a {
    color: var(--accent);
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 0.8rem;
  }
</style>
