import { invoke } from "@tauri-apps/api/core";
import { timeZone } from "./timezone.svelte";

export type ExchField = "rst" | "serial" | "number" | "locator";

export interface CategoryField {
  key: string;
  label: string;
  options: string[];
  default: string;
}

export interface ContestDef {
  id: string;
  name: string;
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
  serial_per_band: boolean;
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
    kind: "never" | "block" | "interval";
    minutes: number;
    per: string[];
    new_square_resets: boolean;
  };
  score_groups: { name: string; bands: string[] }[];
  categories: CategoryField[];
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
  soapbox: string;
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
}

export interface QsoResult {
  dupe: boolean;
  invalid: string | null;
  warning: string | null;
  points: number;
  new_mult: boolean;
  mult: string;
  prefix: string;
  block: number;
  distance_km: number | null;
  next_ok: string | null;
}

export type QsoRow = Qso & QsoResult;

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
  rcvd_exch: string;
  rcvd_locator: string;
}

export interface CheckResult {
  result: QsoResult;
  sent_nr: string;
  known_locator: string | null;
  known_exch: string | null;
  previous: QsoRow[];
}

export type RigKind = "none" | "rigctld" | "flrig" | "tci";

export interface RigConfig {
  kind: RigKind;
  host: string;
  port: number;
  poll_ms: number;
  spots: boolean;
}

export interface RigStatus {
  connected: boolean;
  freq_hz: number | null;
  band: string | null;
  mode: string | null;
  rig_mode: string | null;
  error: string | null;
}

export interface Settings {
  station: Station;
  rig: RigConfig;
  last_log: string | null;
}

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
    invoke<LogView>("new_log", { path, setup }),
  openLog: (path: string) => invoke<LogView>("open_log", { path }),
  closeLog: () => invoke<void>("close_log"),
  getLog: () => invoke<LogView | null>("get_log"),
  updateSetup: (setup: LogSetup) => invoke<LogView>("update_setup", { setup }),
  checkQso: (input: QsoInput) => invoke<CheckResult>("check_qso", { input }),
  logQso: (input: QsoInput) => invoke<LogView>("log_qso", { input }),
  updateQso: (qso: Qso) => invoke<LogView>("update_qso", { qso }),
  deleteQso: (id: number) => invoke<LogView>("delete_qso", { id }),
  cabrilloText: () => invoke<string>("cabrillo_text"),
  exportCabrillo: (path: string) => invoke<void>("export_cabrillo", { path }),
  exportAdif: (path: string) => invoke<void>("export_adif", { path }),
  exportScoreSheet: (path: string) =>
    invoke<void>("export_score_sheet", { path }),
  rigConnect: (config: RigConfig) => invoke<void>("rig_connect", { config }),
  rigDisconnect: () => invoke<void>("rig_disconnect"),
  rigStatus: () => invoke<RigStatus>("rig_status"),
  appVersion: () => invoke<string>("app_version"),
};

export const RIG_STATUS_EVENT = "rig-status";

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
