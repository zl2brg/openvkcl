const KEY = "openvkcl.localTime";

/** Whether times are shown in the computer's local time zone instead of UTC. Logs and exports are always UTC. */
export const timeZone = $state({ local: localStorage.getItem(KEY) === "1" });

export function setLocalTime(local: boolean) {
  timeZone.local = local;
  localStorage.setItem(KEY, local ? "1" : "0");
}
