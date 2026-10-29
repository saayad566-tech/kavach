// Shared type predicates.
//
// Small on purpose: every module needs to agree on what counts as a number, and a single
// definition is the only way to keep `null` (no data) distinct from `0` (a real zero).

/** True for a real, finite number. Strings that look numeric are not numbers. */
export const isNum = (value) => typeof value === 'number' && Number.isFinite(value);

/** True for an empty value in any of the shapes an empty CSV cell can take. */
export const isBlank = (value) => value === null || value === undefined || value === '';
