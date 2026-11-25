<script lang="ts">
  import { untrack } from "svelte";
  import Modal from "./Modal.svelte";
  import { isLocator, pad, type Bonus, type ContestDef, type LogSetup } from "$lib/api";

  let {
    contests,
    initial,
    isNew,
    onsave,
    onclose,
  }: {
    contests: ContestDef[];
    initial: LogSetup;
    isNew: boolean;
    onsave: (setup: LogSetup) => void;
    onclose: () => void;
  } = $props();

  let setup = $state<LogSetup>(untrack(() => $state.snapshot(initial)) as LogSetup);
  const startDate0 = new Date(setup.start);
  let startDate = $state(
    `${startDate0.getUTCFullYear()}-${pad(startDate0.getUTCMonth() + 1)}-${pad(startDate0.getUTCDate())}`,
  );
  let startTime = $state(`${pad(startDate0.getUTCHours())}:${pad(startDate0.getUTCMinutes())}`);
  let error = $state("");

  let contest = $derived(contests.find((c) => c.id === setup.contest_id));
  let general = $derived(!!contest?.general);
  let generic = $derived(!!contest?.generic);
  // General logging first, then a generic contest, then the named contests.
  let choices = $derived(
    [...contests].sort((a, b) => Number(b.general) - Number(a.general) || Number(b.generic) - Number(a.generic)),
  );
  let bandBonuses = $derived(contest?.scoring.bonus.filter((b) => b.per_band) ?? []);

  /** Bands ticked for a per-band bonus. Without a list (older logs) the category claims every band. */
  function bonusBands(b: Bonus): string[] {
    return setup.bonus_bands[b.category] ?? (setup.categories[b.category] === b.value ? (contest?.bands ?? []) : []);
  }

  function toggleBonusBand(b: Bonus, band: string, on: boolean) {
    const bands = bonusBands(b).filter((x) => x !== band);
    setup.bonus_bands[b.category] = on ? [...bands, band] : bands;
  }

  /** The local-time points rule, if this contest has one (only RD at present). The offset is used for nothing else. */
  let localTime = $derived(contest?.scoring.points.local_time ?? null);

  /** Where each offset is used, for the VK/ZL region. Daylight-saving offsets are listed separately. */
  const places: Record<number, string> = {
    0: "UTC",
    480: "WA (AWST)",
    525: "Eucla",
    570: "SA, NT (ACST)",
    600: "QLD, NSW, VIC, TAS, ACT (AEST)",
    630: "SA (ACDT), Lord Howe Is.",
    660: "NSW, VIC, TAS, ACT (AEDT), Lord Howe Is. (summer)",
    690: "Norfolk Is.",
    720: "New Zealand (NZST), Norfolk Is. (summer)",
    765: "Chatham Is.",
    780: "New Zealand (NZDT)",
    825: "Chatham Is. (summer)",
  };
  // Half-hour steps from UTC-12 to UTC+14, plus the region's 45-minute zones and the log's own value.
  let offsets = $derived(
    [
      ...new Set([...Array.from({ length: 53 }, (_, i) => (i - 24) * 30), 525, 765, 825, setup.utc_offset_minutes]),
    ].sort((a, b) => a - b),
  );
  function offsetLabel(m: number) {
    const sign = m < 0 ? "-" : "+";
    const a = Math.abs(m);
    const utc = `UTC${sign}${pad(Math.floor(a / 60))}:${pad(a % 60)}`;
    return places[m] ? `${utc} — ${places[m]}` : utc;
  }

  /**
   * For a new log, default to this computer's UTC offset at the contest start rather than today's, so a log set up
   * in summer for a winter contest gets standard time. Stops once the operator picks an offset themselves.
   */
  let offsetChosen = false;
  function defaultOffset() {
    if (!isNew || offsetChosen) return;
    const start = new Date(`${startDate}T${startTime}:00Z`);
    if (!isNaN(start.getTime())) setup.utc_offset_minutes = -start.getTimezoneOffset();
  }
  defaultOffset();

  function contestChanged() {
    const c = contests.find((x) => x.id === setup.contest_id);
    if (!c) return;
    startTime = c.default_start_utc;
    setup.serial_per_band = c.serial_per_band;
    setup.categories = Object.fromEntries(c.categories.map((f) => [f.key, f.default]));
    setup.bonus_bands = {};
    if (c.generic) {
      setup.generic = {
        name: "",
        duration_minutes: c.duration_minutes,
        bands: [...c.bands],
        cipher: "",
        cipher_label: "",
        include_rst: true,
      };
    }
    defaultOffset();
  }

  /** The contest restarts serials at 001 on each band for this category choice. */
  function wantsSeparateSerials(): boolean {
    const when = contest?.serial_per_band_when;
    const op = setup.categories["CATEGORY-OPERATOR"];
    const tx = setup.categories["CATEGORY-TRANSMITTER"];
    if (when === "multi_op") return op === "MULTI-OP";
    if (when === "multi_tx") return op === "MULTI-OP" && (tx === "TWO" || tx === "UNLIMITED");
    return false;
  }

  /** Follow the contest rule when the operator or transmitter category changes. A manual tick stays until then. */
  function applySerialRule() {
    if (!contest || contest.serial_per_band) return;
    setup.serial_per_band = wantsSeparateSerials();
  }

  function save() {
    error = "";
    // A general log has no contest period; its start is just when it was created.
    const start = general && isNew ? new Date() : new Date(`${startDate}T${startTime}:00Z`);
    if (isNaN(start.getTime())) {
      error = "Enter a valid start date and time.";
      return;
    }
    if (!setup.station.callsign.trim()) {
      error = "Enter your callsign.";
      return;
    }
    if (contest?.exchange.includes("number") && !/^\d+$/.test(setup.sent_number.trim())) {
      error = `Enter your ${contest.number_label.toLowerCase()} to send.`;
      return;
    }
    if (generic) {
      setup.generic.name = setup.generic.name.trim();
      setup.generic.cipher_label = setup.generic.cipher_label.trim();
      if (!setup.generic.name) {
        error = "Enter a name for the contest.";
        return;
      }
      if (setup.generic.duration_minutes <= 0) {
        error = "Enter how long the contest runs.";
        return;
      }
      setup.generic.bands = (contest?.bands ?? []).filter((b) => setup.generic.bands.includes(b));
      if (!setup.generic.bands.length) {
        error = "Choose at least one band.";
        return;
      }
      if (setup.generic.cipher === "text" && !setup.generic.cipher_label) {
        error = "Enter a name for the exchange, such as Temp or Shire.";
        return;
      }
    }
    const loc = setup.station.locator.trim();
    if (!isLocator(loc) || loc.length < 6) {
      error = "Enter your grid locator: 6 characters, or 8 or 10 for more precision.";
      return;
    }
    const st = setup.station;
    const missing = (
      [
        ["name", st.name],
        ["email", st.email],
        ["city", st.city],
        ["state", st.state],
        ["country", st.country],
      ] as const
    )
      .filter(([, v]) => !v.trim())
      .map(([k]) => k);
    if (missing.length) {
      error = `Enter your ${missing.join(", ")}.`;
      return;
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(st.email.trim())) {
      error = "Enter a valid email address.";
      return;
    }
    st.name = st.name.trim();
    st.email = st.email.trim();
    st.city = st.city.trim();
    st.state = st.state.trim();
    st.country = st.country.trim();
    setup.station.locator = loc;
    setup.start = start.toISOString();
    setup.station.callsign = setup.station.callsign.trim().toUpperCase();
    setup.station.operators = setup.station.operators.trim().toUpperCase();
    setup.sent_number = setup.sent_number.trim();
    onsave($state.snapshot(setup) as LogSetup);
  }
</script>

<Modal title={isNew ? "New log" : general ? "Log setup" : "Contest setup"} {onclose} wide>
  <div class="grid">
    <section>
      <h3>{general ? "Log" : "Contest"}</h3>
      <label>
        Log type
        <select bind:value={setup.contest_id} onchange={contestChanged} disabled={!isNew}>
          {#each choices as c (c.id)}
            <option value={c.id}>{c.name}</option>
          {/each}
        </select>
      </label>
      {#if general}
        <p class="hint">
          For everyday operating and chasing DX: no contest period, scoring or dupes. Shows each station's country and
          whether it's a new one for you. Import your existing log with Import ADIF once it's created.
        </p>
      {:else if generic}
        <p class="hint">
          For a contest that is not in the list. Choose the exchange. Nothing is marked a dupe and nothing is scored;
          Cabrillo carries the contacts to the contest manager, with the contact count as the claimed score.
        </p>
      {:else if contest?.rules_url}
        <p class="hint">Rules: {contest.rules_url}</p>
      {/if}
      {#if !general}
        <div class="row">
          <label>
            Start date (UTC)
            <input type="date" bind:value={startDate} onchange={defaultOffset} />
          </label>
          <label>
            Start time (UTC)
            <input type="time" bind:value={startTime} onchange={defaultOffset} />
          </label>
        </div>
        {#if generic}
          <label class="plain">
            Contest name
            <input bind:value={setup.generic.name} placeholder="e.g. QRP Hours" />
          </label>
          <label>
            Length (hours)
            <input
              type="number"
              min="0.5"
              step="0.5"
              value={setup.generic.duration_minutes / 60}
              onchange={(e) => (setup.generic.duration_minutes = Math.round(Number(e.currentTarget.value) * 60))}
            />
          </label>
          <div class="bands">
            Bands
            <div class="ticks">
              {#each contest?.bands ?? [] as b (b)}
                <label class="check">
                  <input
                    type="checkbox"
                    checked={setup.generic.bands.includes(b)}
                    onchange={(e) => {
                      const on = e.currentTarget.checked;
                      setup.generic.bands = on
                        ? [...setup.generic.bands, b]
                        : setup.generic.bands.filter((x) => x !== b);
                    }}
                  />
                  {b}
                </label>
              {/each}
            </div>
          </div>
          <label class="plain">
            Exchange
            <select bind:value={setup.generic.cipher}>
              <option value="">None</option>
              <option value="serial">Serial number</option>
              <option value="locator">Grid locator</option>
              <option value="text">Text</option>
            </select>
          </label>
          {#if setup.generic.cipher === "text"}
            <label class="plain">
              Exchange name
              <input bind:value={setup.generic.cipher_label} placeholder="e.g. Temp or Shire" />
            </label>
          {/if}
          <label class="check">
            <input type="checkbox" bind:checked={setup.generic.include_rst} />
            Exchange RST
          </label>
        {/if}
        {#if localTime}
          <label>
            Your local time zone during the contest
            <select bind:value={setup.utc_offset_minutes} onchange={() => (offsetChosen = true)}>
              {#each offsets as o (o)}
                <option value={o}>{offsetLabel(o)}</option>
              {/each}
            </select>
          </label>
          <p class="hint">
            Contacts from {localTime.from} to {localTime.to} your local time score ×{localTime.factor}.
          </p>
        {/if}
      {/if}

      <h3>{general ? "Station" : "Exchange"}</h3>
      <div class="row">
        <label>
          Callsign
          <input class="mono" bind:value={setup.station.callsign} autocapitalize="characters" />
        </label>
        <label>
          Grid locator *
          <input class="mono" bind:value={setup.station.locator} maxlength="10" />
        </label>
      </div>
      {#if contest?.exchange.includes("number")}
        <label>
          {contest.number_label} (sent)
          <input class="mono" bind:value={setup.sent_number} inputmode="numeric" />
        </label>
      {/if}
      {#if generic && setup.generic.cipher === "serial"}
        <label class="check">
          <input type="checkbox" bind:checked={setup.serial_per_band} />
          Separate serial numbers on each band
        </label>
      {:else if contest?.serial_per_band}
        <p class="hint">Serial numbers run separately on each band in this contest.</p>
      {:else if contest?.exchange.includes("serial")}
        <label class="check">
          <input type="checkbox" bind:checked={setup.serial_per_band} />
          Separate serial numbers on each band
        </label>
        {#if wantsSeparateSerials()}
          <p class="hint">
            {contest?.serial_per_band_when === "multi_tx"
              ? "Multi-transmitter stations start serial numbers at 001 on each band."
              : "Multi-operator stations start serial numbers at 001 on each band."}
          </p>
        {/if}
      {/if}

      {#if contest?.categories.length}<h3>Categories</h3>{/if}
      {#each contest?.categories ?? [] as cat (cat.key)}
        <label class:plain={!!cat.label}>
          {cat.label || cat.key.replace("CATEGORY-", "").toLowerCase()}
          <select
            bind:value={setup.categories[cat.key]}
            onchange={(e) => {
              if (cat.key === "CATEGORY-OPERATOR" || cat.key === "CATEGORY-TRANSMITTER") {
                setup.categories[cat.key] = e.currentTarget.value;
                applySerialRule();
              }
            }}
          >
            {#each cat.options as opt (opt)}
              <option value={opt}>{opt || "(none)"}</option>
            {/each}
          </select>
        </label>
      {/each}
    </section>

    <section>
      <h3>{general ? "Station details" : "Station details (Cabrillo header)"}</h3>
      {#if !general}
        <label>Operators <input class="mono" bind:value={setup.station.operators} placeholder="VK3ABC VK3XYZ" /></label>
      {/if}
      <label>Name * <input bind:value={setup.station.name} /></label>
      <label>Email * <input type="email" bind:value={setup.station.email} /></label>
      <label>Club <input bind:value={setup.station.club} /></label>
      <label>Address <input bind:value={setup.station.address} /></label>
      <div class="row">
        <label>City * <input bind:value={setup.station.city} /></label>
        <label>State * <input bind:value={setup.station.state} /></label>
      </div>
      <div class="row">
        <label>Postcode <input bind:value={setup.station.postcode} /></label>
        <label>Country * <input bind:value={setup.station.country} /></label>
      </div>
      {#if !general}
        <label>Location <input bind:value={setup.location} placeholder="e.g. VIC" /></label>
        <label>Soapbox <textarea rows="3" bind:value={setup.soapbox}></textarea></label>
      {/if}
      {#each bandBonuses as b (b.category)}
        <div class="bands">
          {b.label || `${b.value} x${b.factor} on`}
          <div class="ticks">
            {#each contest?.bands ?? [] as band (band)}
              <label class="check">
                <input
                  type="checkbox"
                  checked={bonusBands(b).includes(band)}
                  onchange={(e) => toggleBonusBand(b, band, e.currentTarget.checked)}
                />
                {band}
              </label>
            {/each}
          </div>
        </div>
      {/each}
    </section>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" onclick={save}>{isNew ? "Create log…" : "Save"}</button>
  {/snippet}
</Modal>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--muted);
  }
  h3:first-child {
    margin-top: 0;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.9rem;
    font-weight: 600;
    text-transform: capitalize;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    text-transform: none;
  }
  label.plain {
    text-transform: none;
  }
  .bands {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 0.85rem;
  }
  .ticks {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .hint {
    margin: 0;
    font-size: 0.8rem;
    color: var(--muted);
    word-break: break-all;
  }
</style>
