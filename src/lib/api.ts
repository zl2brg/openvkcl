import { invoke } from "@tauri-apps/api/core";
import { timeZone } from "./timezone.svelte";

export type ExchField = "rst" | "serial" | "number" | "locator" | "text";

export interface CategoryField {
  key: string;
  label: string;
  options: string[];
  default: string;
}

export interface ContestDef {
  id: string;
  name: string;
  /** A general (DX) log: no contest period, scoring or dupes. */
  general: boolean;
  /** The operator names it and chooses the exchange. No dupes and no score. */
  generic: boolean;
  cabrillo_name: string;
  rules_url: string;
  duration_minutes: number;
  default_start_utc: string;
  bands: string[];
  modes: string[];
  mode_groups: Record<string, string>;
  exchange: ExchField[];
  number_label: string;
  number_from: string[];
  /** An official list of codes for the number field (e.g. VK Shires), read from the contests folder. */
  number_list: NumberList | null;
  /** Listed codes and what they stand for, e.g. BU4 -> "Bundaberg, Qld". Empty if the list couldn't be read. */
  number_names: Record<string, string>;
  /** Why the list couldn't be read, e.g. the file isn't there yet. */
  number_list_error: string | null;
  serial_per_band: boolean;
  /** When the setup dialog should tick separate serials per band. */
  serial_per_band_when: "never" | "multi_op" | "multi_tx";
  workable: string[];
  blocks: { minutes: number } | null;
  sessions: { start_minute: number; minutes: number }[];
  multiplier: {
    kind: "prefix" | "number";
    per: string[];
    only: string[];
    exclude: string[];
    not_own: boolean;
  } | null;
  rework: {
    kind: "never" | "block" | "interval" | "off";
    minutes: number;
    per: string[];
    new_square_resets: boolean;
  };
  score_groups: { name: string; bands: string[] }[];
  categories: CategoryField[];
  scoring: {
    bonus: Bonus[];
    /** Only "fixed" points have `local_time`: a points factor between two local times (e.g. RD's 01:00-06:00 x3). */
    points: { kind: string; local_time?: { from: string; to: string; factor: number } | null };
  };
}

export interface NumberList {
  file: string;
  /** What one code is called, e.g. "shire". */
  item: string;
  /** CQ zones 1-40 are valid too. */
  cq_zones: boolean;
  url: string;
}

/**
 * What a number-field code stands for in a contest with a code list: the listed name, "CQ zone N", or "" when it
 * isn't valid. `null` when there's nothing to check against (no list, or it couldn't be read).
 */
export function numberMeaning(contest: ContestDef, code: string): string | null {
  const list = contest.number_list;
  if (!list) return null;
  const c = code.trim().toUpperCase();
  const listed = contest.number_names[c];
  if (listed) return listed;
  if (list.cq_zones && /^\d+$/.test(c) && +c >= 1 && +c <= 40) return `CQ zone ${+c}`;
  return Object.keys(contest.number_names).length ? "" : null;
}

export interface Bonus {
  category: string;
  value: string;
  factor: number;
  per_band: boolean;
  label: string;
}

export interface Station {
  callsign: string;
  operators: string;
  name: string;
  email: string;
  address: string;
  city: string;
  state: string;
  postcode: string;
  country: string;
  club: string;
  locator: string;
}

export interface LogSetup {
  contest_id: string;
  start: string;
  station: Station;
  sent_number: string;
  serial_per_band: boolean;
  utc_offset_minutes: number;
  location: string;
  categories: Record<string, string>;
  bonus_bands: Record<string, string[]>;
  soapbox: string;
  /** Choices for a generic contest. */
  generic: GenericOptions;
}

export interface GenericOptions {
  name: string;
  duration_minutes: number;
  bands: string[];
  /** "serial", "locator", "text", or "" for none. */
  cipher: string;
  cipher_label: string;
  include_rst: boolean;
}

export interface Qso {
  id: number;
  time: string;
  call: string;
  band: string;
  mode: string;
  freq_khz: number | null;
  sent_rst: string;
  sent_nr: string;
  sent_locator: string;
  rcvd_rst: string;
  rcvd_nr: string;
  rcvd_locator: string;
  operator: string;
  sent_exch: string;
  rcvd_exch: string;
  name: string;
  qth: string;
  comment: string;
  /** ADIF QSL status: Y, N, R (requested), Q (queued), I (ignore) or "". */
  qsl_sent: string;
  qsl_rcvd: string;
  lotw_sent: string;
  lotw_rcvd: string;
  eqsl_sent: string;
  eqsl_rcvd: string;
}

/** The country a station is in, from the country file. */
export interface Country {
  dxcc: number;
  country: string;
  cont: string;
  cqz: number;
  ituz: number;
}

export interface QsoResult {
  dupe: boolean;
  invalid: string | null;
  warning: string | null;
  points: number;
  new_mult: boolean;
  mult: string;
  distance_km: number | null;
  next_ok: string | null;
  rework_at: string | null;
}

/** In general logs, rows carry the station's country and whether it's the first contact with it. */
export type QsoRow = Qso & QsoResult & Partial<Country> & { new_dxcc: boolean };

/**
 * Values the backend leaves out of a row when they're empty or default, to keep
 * a long log small to send; `fillRow` puts them back.
 */
const ROW_DEFAULTS = {
  freq_khz: null,
  sent_nr: "",
  sent_locator: "",
  rcvd_nr: "",
  rcvd_locator: "",
  operator: "",
  sent_exch: "",
  rcvd_exch: "",
  name: "",
  qth: "",
  comment: "",
  qsl_sent: "",
  qsl_rcvd: "",
  lotw_sent: "",
  lotw_rcvd: "",
  eqsl_sent: "",
  eqsl_rcvd: "",
  new_dxcc: false,
} as const;

const RESULT_DEFAULTS: QsoResult = {
  dupe: false,
  invalid: null,
  warning: null,
  points: 0,
  new_mult: false,
  mult: "",
  distance_km: null,
  next_ok: null,
  rework_at: null,
};

export function fillRow(r: Partial<QsoRow>): QsoRow {
  return { ...ROW_DEFAULTS, ...RESULT_DEFAULTS, ...r } as QsoRow;
}

function fillView(v: LogView): LogView {
  return { ...v, qsos: v.qsos.map(fillRow) };
}

/** Logging a contact: the new row to add at the end, or the whole log. */
export type Logged = { kind: "row"; row: QsoRow } | { kind: "view"; view: LogView };

/** Editing or deleting a contact: the rows that changed (and any removed), or the whole log. */
export type Changed = { kind: "rows"; rows: QsoRow[]; removed: number | null } | { kind: "view"; view: LogView };

function fillChanged(c: Changed): Changed {
  return c.kind === "rows" ? { ...c, rows: c.rows.map(fillRow) } : { kind: "view", view: fillView(c.view) };
}

/** The log with `change` applied: changed rows replaced in place, a deleted one removed. */
export function applyChange(view: LogView, change: Changed): LogView {
  if (change.kind === "view") return change.view;
  const changed = new Map(change.rows.map((r) => [r.id, r]));
  const qsos = view.qsos.filter((q) => q.id !== change.removed).map((q) => changed.get(q.id) ?? q);
  return { ...view, qsos };
}

export interface GroupScore {
  name: string;
  qsos: number;
  points: number;
  mults: number;
  score: number;
}

export interface Score {
  groups: GroupScore[];
  total: number;
  qsos: number;
  dupes: number;
  invalid: number;
  by_band: [string, number][];
}

export interface LogView {
  path: string;
  setup: LogSetup;
  contest: ContestDef;
  qsos: QsoRow[];
  score: Score;
}

export interface QsoInput {
  time: string | null;
  call: string;
  band: string;
  mode: string;
  freq_khz: number | null;
  sent_rst: string;
  rcvd_rst: string;
  rcvd_nr: string;
  sent_exch: string;
  rcvd_exch: string;
  rcvd_locator: string;
  name: string;
  qth: string;
  comment: string;
}

/** Whether a country has been worked before (on a band, in a mode class). */
export type Need = "new" | "worked" | "confirmed";

export interface DxCheck {
  country: Country;
  any: Need;
  band: Need;
  mode: Need;
  /** The country on each band and mode class it has been worked on. */
  slots: { band: string; class: "PH" | "CW" | "DG"; need: Need }[];
}

export interface CheckResult {
  result: QsoResult;
  sent_nr: string;
  known_locator: string | null;
  known_exch: string | null;
  known_name: string | null;
  dx: DxCheck | null;
  previous: QsoRow[];
}

export interface CtyStatus {
  count: number;
  path: string;
  updated: string | null;
}

export interface ImportSummary {
  imported: number;
  duplicates: number;
  out_of_range: number;
  not_in_contest: number;
  unreadable: number;
}

export interface ImportResult {
  summary: ImportSummary;
  view: LogView;
}

export type RigKind = "none" | "hamlib" | "rigctld" | "flrig" | "tci" | "flex";

export interface FlexRadio {
  ip: string;
  port: number;
  model: string;
  nickname: string;
  callsign: string;
  serial: string;
}

export interface RigConfig {
  kind: RigKind;
  host: string;
  port: number;
  poll_ms: number;
  spots: boolean;
  model: number;
  serial_port: string;
  baud: number;
  rigctld_args: string;
}

export interface RigModel {
  id: number;
  mfg: string;
  model: string;
}

export interface SerialPort {
  path: string;
  label: string;
}

export interface RigStatus {
  connected: boolean;
  freq_hz: number | null;
  band: string | null;
  mode: string | null;
  rig_mode: string | null;
  error: string | null;
}

/** A voice keyer message: its label and its .wav file. */
export interface KeyerMessage {
  label: string;
  /** A name in the voice folder, or a full path; empty when not recorded yet. */
  file: string;
}

export interface KeyerConfig {
  /** Sound output that feeds the radio; empty uses the system default. */
  output: string;
  /** Microphone to record from; empty uses the system default. */
  input: string;
  /** Key the radio through its connection; otherwise rely on its VOX. */
  ptt: boolean;
  ptt_delay_ms: number;
  /** Pause between the end of one CQ and the next when repeating. */
  repeat_ms: number;
  /** Run and S&P messages, F1 to F12. */
  run: KeyerMessage[];
  sp: KeyerMessage[];
}

export interface KeyerStatus {
  playing: string | null;
  recording: string | null;
  /** The file that just finished playing on its own, for CQ repeat. */
  finished: string | null;
  error: string | null;
}

export interface KeyerInfo {
  devices: { outputs: string[]; inputs: string[] };
  voice_dir: string;
  status: KeyerStatus;
}

export type KeySet = "run" | "sp";

/** The file a key records to in the voice folder, e.g. "run-F1.wav". */
export function recordingName(set: KeySet, key: number): string {
  return `${set}-F${key + 1}.wav`;
}

export const KEYER_STATUS_EVENT = "keyer-status";
export const KEYER_LEVEL_EVENT = "keyer-level";

export interface QrzConfig {
  enabled: boolean;
  username: string;
  password: string;
}

export interface QrzInfo {
  call: string;
  name: string;
  city: string;
  state: string;
  country: string;
  grid: string;
  email: string;
  lat: number | null;
  lon: number | null;
}

export interface ScpStatus {
  count: number;
  path: string;
  updated: string | null;
}

export interface ScpMatches {
  exact: boolean;
  partial: string[];
  near: string[];
}

export interface Settings {
  station: Station;
  rig: RigConfig;
  keyer: KeyerConfig;
  qrz: QrzConfig;
  cluster: ClusterConfig;
  last_log: string | null;
}

export interface ClusterConfig {
  enabled: boolean;
  host: string;
  port: number;
  /** Login callsign; empty uses the station callsign. */
  callsign: string;
}

export interface ClusterStatus {
  connected: boolean;
  /** "host:port", while enabled. */
  node: string;
  error: string | null;
}

/** A DX cluster spot, with the spotted station's country when it's known. */
export type Spot = {
  time: string;
  freq_khz: number;
  call: string;
  spotter: string;
  comment: string;
  band: string | null;
} & Partial<Country>;

export interface ContestList {
  contests: ContestDef[];
  errors: string[];
  user_dir: string;
}

export const api = {
  listContests: () => invoke<ContestList>("list_contests"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    invoke<void>("save_settings", { settings }),
  newLog: (path: string, setup: LogSetup) =>
    invoke<LogView>("new_log", { path, setup }).then(fillView),
  openLog: (path: string) => invoke<LogView>("open_log", { path }).then(fillView),
  closeLog: () => invoke<void>("close_log"),
  getLog: () => invoke<LogView | null>("get_log").then((v) => v && fillView(v)),
  updateSetup: (setup: LogSetup) => invoke<LogView>("update_setup", { setup }).then(fillView),
  checkQso: (input: QsoInput) =>
    invoke<CheckResult>("check_qso", { input }).then((c) => ({
      ...c,
      result: { ...RESULT_DEFAULTS, ...c.result },
      previous: c.previous.map(fillRow),
    })),
  logQso: (input: QsoInput) =>
    invoke<Logged>("log_qso", { input }).then(
      (l): Logged => (l.kind === "row" ? { kind: "row", row: fillRow(l.row) } : { kind: "view", view: fillView(l.view) }),
    ),
  updateQso: (qso: Qso) => invoke<Changed>("update_qso", { qso }).then(fillChanged),
  deleteQso: (id: number) => invoke<Changed>("delete_qso", { id }).then(fillChanged),
  cabrilloText: () => invoke<string>("cabrillo_text"),
  exportCabrillo: (path: string) => invoke<void>("export_cabrillo", { path }),
  /** Returns how many contacts were exported. */
  exportAdif: (path: string, from: string | null, to: string | null) =>
    invoke<number>("export_adif", { path, from, to }),
  importAdif: (path: string, from: string | null, to: string | null) =>
    invoke<ImportResult>("import_adif", { path, from, to }).then((r) => ({ ...r, view: fillView(r.view) })),
  ctyStatus: () => invoke<CtyStatus>("cty_status"),
  ctyDownload: () => invoke<CtyStatus>("cty_download"),
  ctyImport: (path: string) => invoke<CtyStatus>("cty_import", { path }),
  exportScoreSheet: (path: string) =>
    invoke<void>("export_score_sheet", { path }),
  rigConnect: (config: RigConfig) => invoke<void>("rig_connect", { config }),
  rigDisconnect: () => invoke<void>("rig_disconnect"),
  rigStatus: () => invoke<RigStatus>("rig_status"),
  keyerInfo: () => invoke<KeyerInfo>("keyer_info"),
  /** Key the radio and play a message file. */
  keyerPlay: (file: string) => invoke<void>("keyer_play", { file }),
  /** Stop playing (unkeying the radio) or recording. */
  keyerStop: () => invoke<void>("keyer_stop"),
  /** Record from the microphone into a file in the voice folder until stopped. */
  keyerRecord: (file: string) => invoke<void>("keyer_record", { file }),
  rigModels: () => invoke<RigModel[]>("rig_models"),
  flexDiscover: () => invoke<FlexRadio[]>("flex_discover"),
  serialPorts: () => invoke<SerialPort[]>("serial_ports"),
  appVersion: () => invoke<string>("app_version"),
  scpStatus: () => invoke<ScpStatus>("scp_status"),
  scpDownload: () => invoke<ScpStatus>("scp_download"),
  scpImport: (path: string) => invoke<ScpStatus>("scp_import", { path }),
  scpLookup: (partial: string) => invoke<ScpMatches>("scp_lookup", { partial }),
  qrzTest: (config: QrzConfig) => invoke<string>("qrz_test", { config }),
  qrzLookup: (call: string) => invoke<QrzInfo | null>("qrz_lookup", { call }),
  /** Save the cluster settings and reconnect. */
  clusterConnect: (config: ClusterConfig) => invoke<ClusterStatus>("cluster_connect", { config }),
  clusterStatus: () => invoke<ClusterStatus>("cluster_status"),
  clusterSpots: () => invoke<Spot[]>("cluster_spots"),
};

export const RIG_STATUS_EVENT = "rig-status";
export const CLUSTER_SPOT_EVENT = "cluster-spot";
export const CLUSTER_STATUS_EVENT = "cluster-status";

export function emptyStation(): Station {
  return {
    callsign: "",
    operators: "",
    name: "",
    email: "",
    address: "",
    city: "",
    state: "",
    postcode: "",
    country: "",
    club: "",
    locator: "",
  };
}

/** "UTC" or "local", for labelling times shown with the helpers below. */
export function zoneLabel(): string {
  return timeZone.local ? "local" : "UTC";
}

function parts(d: Date) {
  return timeZone.local
    ? [
        d.getFullYear(),
        d.getMonth() + 1,
        d.getDate(),
        d.getHours(),
        d.getMinutes(),
        d.getSeconds(),
      ]
    : [
        d.getUTCFullYear(),
        d.getUTCMonth() + 1,
        d.getUTCDate(),
        d.getUTCHours(),
        d.getUTCMinutes(),
        d.getUTCSeconds(),
      ];
}

/** "2026-08-15T03:05:00Z" -> "0305", in the display time zone. */
export function hhmm(iso: string): string {
  const [, , , h, m] = parts(new Date(iso));
  return `${pad(h)}${pad(m)}`;
}

/** How long until `iso`, e.g. "in 60 mins", "in 1 h 15 mins", or "now". */
export function untilText(iso: string, now: Date = new Date()): string {
  const mins = Math.ceil((new Date(iso).getTime() - now.getTime()) / 60_000);
  if (mins <= 0) return "now";
  if (mins < 60) return mins === 1 ? "in 1 min" : `in ${mins} mins`;
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (m === 0) return h === 1 ? "in 1 hour" : `in ${h} hours`;
  return `in ${h} h ${m} min${m === 1 ? "" : "s"}`;
}

/** Time left until `iso` as "2:59:41" or "4:05", or null once it has passed. */
export function countdownText(iso: string, now: Date = new Date()): string | null {
  const secs = Math.ceil((new Date(iso).getTime() - now.getTime()) / 1000);
  if (secs <= 0) return null;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return h ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** "13:05:09" in the display time zone. */
export function clockText(d: Date): string {
  const [, , , h, m, s] = parts(d);
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}

/** "2026-08-15T03:05:00Z" -> "2026-08-15 03:05", in the display time zone. */
export function dateTimeText(iso: string): string {
  const [y, mo, d, h, m] = parts(new Date(iso));
  return `${y}-${pad(mo)}-${pad(d)} ${pad(h)}:${pad(m)}`;
}

/** Parse "YYYY-MM-DD HH:MM" in the display time zone to an ISO string, or null if malformed. */
export function parseDateTimeText(text: string): string | null {
  const m = text
    .trim()
    .match(/^(\d{4})-(\d{2})-(\d{2})[ T](\d{1,2}):?(\d{2})$/);
  if (!m) return null;
  const f = [+m[1], +m[2] - 1, +m[3], +m[4], +m[5]] as const;
  const d = timeZone.local ? new Date(...f) : new Date(Date.UTC(...f));
  return isNaN(d.getTime()) ? null : d.toISOString();
}

/** "2026-08-15T03:05:00Z" -> "2026-08-15", in the display time zone. */
export function dateText(iso: string): string {
  const [y, mo, d] = parts(new Date(iso));
  return `${y}-${pad(mo)}-${pad(d)}`;
}

/**
 * The start (or with `end`, the last millisecond) of a "YYYY-MM-DD" day in the
 * display time zone, as an ISO string; null for an empty or malformed date.
 */
export function dayBound(date: string, end = false): string | null {
  const m = date.trim().match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!m) return null;
  const f = [+m[1], +m[2] - 1, +m[3] + (end ? 1 : 0)] as const;
  const t = timeZone.local ? new Date(...f).getTime() : Date.UTC(...f);
  return isNaN(t) ? null : new Date(end ? t - 1 : t).toISOString();
}

/** A From/To date range of whole days; empty strings leave that end open. */
export interface DateRange {
  from: string;
  to: string;
}

/** A test for whether an ISO time falls inside `range`. */
export function rangeTest(range: DateRange): (iso: string) => boolean {
  const from = dayBound(range.from);
  const to = dayBound(range.to, true);
  const f = from ? new Date(from).getTime() : -Infinity;
  const t = to ? new Date(to).getTime() : Infinity;
  return (iso) => {
    const x = new Date(iso).getTime();
    return x >= f && x <= t;
  };
}

/** Phone, CW or data: the mode classes DXCC awards count by. */
export function modeClass(mode: string): "PH" | "CW" | "DG" {
  if (["PH", "SSB", "USB", "LSB", "AM", "FM", "DIGITALVOICE"].includes(mode)) return "PH";
  return mode === "CW" ? "CW" : "DG";
}

/** Signal report assumed when none is typed; "" for modes that exchange dB reports. */
/**
 * Why `rst` isn't a valid report in `mode`, or null if it is (blank uses the
 * default). Phone, CW and RTTY take 59 or 599. WSJT-X modes report dB like
 * -10, and a contest's DG takes either of those or a dB report.
 */
export function reportProblem(mode: string, rst: string): string | null {
  const r = rst.trim().replace(/\+$/, "");
  if (!rst.trim()) return null;
  const rs = /^[1-5][1-9]$/.test(r);
  const rstOk = /^[1-5][1-9][1-9]$/.test(r);
  const signal = rs || rstOk;
  const db = /^[+-]\d{1,2}$/.test(rst.trim());
  if (mode === "DG") return signal || db ? null : "must be like 59 or 599, or a report in dB like -10";
  if (["RY", "RTTY", "PSK31", "OLIVIA", "SSTV"].includes(mode) || modeClass(mode) === "CW" || modeClass(mode) === "PH") {
    return signal ? null : "must be like 59 or 599";
  }
  return db ? null : "must be a signal report in dB like -10 or +05";
}

export function defaultRst(mode: string): string {
  const c = modeClass(mode);
  if (c === "PH") return "59";
  if (c === "CW" || ["RY", "RTTY", "PSK31", "OLIVIA", "SSTV", "DG"].includes(mode)) return "599";
  return "";
}

/**
 * The general-log mode for what the radio reports (Hamlib/FLRig/TCI names), or
 * null to keep `current`, e.g. on a data mode when a digital mode is already chosen.
 */
export function generalMode(rigMode: string, current: string): string | null {
  const m = rigMode.trim().toUpperCase();
  const data = m.startsWith("PKT") || m.startsWith("DATA") || m.startsWith("DIG") || m.includes("-D");
  if (data) return modeClass(current) === "DG" && current !== "RTTY" ? null : "FT8";
  if (m.startsWith("CW")) return "CW";
  if (m.startsWith("RTTY") || m.startsWith("FSK")) return "RTTY";
  if (["AM", "SAM"].includes(m)) return "AM";
  if (["FM", "WFM", "FMN", "NFM"].includes(m)) return "FM";
  if (["USB", "LSB", "SSB", "DSB"].includes(m)) return "SSB";
  if (["D-STAR", "DSTAR", "DV", "FDV", "FREEDV"].includes(m)) return "DIGITALVOICE";
  return null;
}

/** QSL confirmations that count for DXCC: a paper card or LoTW. */
export function isConfirmed(q: Qso): boolean {
  return q.qsl_rcvd === "Y" || q.lotw_rcvd === "Y";
}

/** Minute-precision entry can sit slightly ahead of wall clock; only warn past this slack. */
const FUTURE_QSO_SLACK_MS = 60_000;

/** True when a parsed QSO time is meaningfully after wall-clock now. */
export function isFutureQsoTime(iso: string | null, now: Date = new Date()): boolean {
  if (!iso) return false;
  return new Date(iso).getTime() - now.getTime() > FUTURE_QSO_SLACK_MS;
}

/** A 4, 6, 8 or 10 character Maidenhead locator, e.g. "QF22", "RF73hc", "RF73hc34". */
export function isLocator(text: string): boolean {
  return /^[A-R]{2}\d{2}([A-X]{2}(\d{2}([A-X]{2})?)?)?$/i.test(text.trim());
}

export function pad(n: number): string {
  return n.toString().padStart(2, "0");
}

export function formatFreq(hz: number | null): string {
  if (hz == null) return "";
  const khz = hz / 1000;
  return khz.toLocaleString("en-AU", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

export function errorText(e: unknown): string {
  return typeof e === "string"
    ? e
    : e instanceof Error
      ? e.message
      : JSON.stringify(e);
}
