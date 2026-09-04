// CALLT targets are selected by their table index, not by method name.  Keep
// the synthetic spelling both inert as source code and collision-free within
// a NEF token table so later renderers can recover the exact token (including
// its contract hash and call flags) without consulting attacker-controlled
// metadata.
const METHOD_TOKEN_LABEL_PREFIX = "__callt_token_";

export function isReservedMethodTokenLabel(identifier) {
  return String(identifier).startsWith(METHOD_TOKEN_LABEL_PREFIX);
}

export function methodTokenFallbackLabel(index) {
  if (!Number.isSafeInteger(index) || index < 0) {
    throw new RangeError("method token index must be a non-negative safe integer");
  }
  return `${METHOD_TOKEN_LABEL_PREFIX}${index}`;
}

export const METHOD_TOKEN_FALLBACK_CALL_PATTERN =
  /\b(__callt_token_([0-9]+))\s*\(/g;
