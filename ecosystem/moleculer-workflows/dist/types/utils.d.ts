/**
 * Parse a duration string into a numeric value based on the specified format.
 *
 * @param {string|number} str The duration string to parse.
 * @param {string} [format="ms"] The format to return the duration in (e.g., "ms", "s").
 * @returns {number|null} The parsed duration in the specified format, or null if invalid.
 */
declare function parseDuration(str: string | number, format?: string): number | null;
/**
 * Generate a random value around the given number, reducing it by up to 25% and adding a random offset.
 *
 * @param {number} x The base value to randomize.
 * @returns {number} The randomized value.
 */
declare function circa(x: number): number;
/**
 * Convert a duration in milliseconds into a human-readable string.
 *
 * @param milli The duration in milliseconds.
 * @returns The human-readable duration (e.g., "1h", "30m").
 */
declare function humanize(milli: number | null): string;
/**
 * Calculate the next execution time for a given cron expression.
 *
 * @param cron The cron expression to evaluate.
 * @param currentDate The current date to base the calculation on.
 * @param tz The timezone to use for the calculation.
 * @returns The timestamp of the next execution time.
 */
declare function getCronNextTime(cron: string, currentDate?: number, tz?: string): number;
export { parseDuration, circa, humanize, getCronNextTime };
//# sourceMappingURL=utils.d.ts.map