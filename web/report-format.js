// JSON has no bigint type. Keep wide VM integers as decimal strings in the
// browser's JSON views rather than rounding them or failing the entire report.
export function stringifyReport(value) {
  return JSON.stringify(value, (_key, item) => typeof item === "bigint" ? item.toString() : item, 2);
}
