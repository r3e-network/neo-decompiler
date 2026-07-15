const STATIC_SLOT_RE = /\bstatic(\d+)\b/g;

/**
 * Collect static VM slots referenced by a high-level method surface.
 *
 * High-level output uses `staticN` as a readable VM slot name. Generated C#
 * keeps those slots at class scope under a reserved prefix so a method-local
 * identifier can never shadow the storage field.
 */
export function staticSlotIndices(lines) {
  const slots = new Set();
  for (const line of lines) {
    if (line.trimStart().startsWith("//")) continue;
    for (const match of scanStaticSlots(line)) slots.add(Number(match));
  }
  return [...slots].sort((left, right) => left - right);
}

export function renderStaticSlotDeclarations(lines, types = null) {
  return staticSlotIndices(lines)
    .map((slot) => `    private static ${types?.get(slot) ?? "dynamic"} ${staticSlotName(slot)};`);
}

/**
 * Infer a stable C# type for each VM static slot from the already-computed
 * per-line declaration maps. A static is promoted only when every assignment
 * is concrete and agrees; reads-only, unknown, null-mixed, and conflicting
 * slots remain dynamic so a VM value is never forced through an invented ABI.
 */
export function inferStaticSlotTypes(lines, typesByLine = null) {
  const states = new Map();
  for (const [lineIndex, line] of lines.entries()) {
    if (line.trimStart().startsWith("//")) continue;
    const assignment = line.trim().match(
      /^(?:let\s+)?static(\d+)\s*=\s*.+;(?:\s*\/\/.*)?$/u,
    );
    if (!assignment) continue;
    const slot = Number(assignment[1]);
    const inferred = typesByLine?.get(lineIndex)?.get(`static${assignment[1]}`) ?? null;
    const state = states.get(slot) ?? { type: null, uncertain: false };
    if (!inferred || inferred === "dynamic" || inferred === "null") {
      state.uncertain = true;
    } else if (state.type && state.type !== inferred) {
      state.uncertain = true;
    } else {
      state.type = inferred;
    }
    states.set(slot, state);
  }
  return new Map(
    [...states.entries()]
      .filter(([, state]) => state.type && !state.uncertain)
      .map(([slot, state]) => [slot, state.type]),
  );
}

/**
 * Rewrite a high-level body line for C#.
 *
 * The first `STSFLD` write is rendered as `let staticN = ...` by the
 * high-level lifter. It is a class-field assignment in C#, so remove only
 * that declaration keyword while rewriting the slot reference itself.
 */
export function renderStaticSlotLine(line) {
  if (line.trimStart().startsWith("//")) return line;
  const rewritten = replaceStaticSlots(line);
  return rewritten.replace(
    /^(\s*)let\s+(__neoStatic\d+)\b/u,
    "$1$2",
  );
}

function staticSlotName(slot) {
  return `__neoStatic${slot}`;
}

function scanStaticSlots(line) {
  const matches = [];
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    if (index > 0 && /[A-Za-z0-9_]/u.test(line[index - 1])) continue;
    const match = line.slice(index).match(/^static(\d+)\b/u);
    if (match) {
      matches.push(match[1]);
      index += match[0].length - 1;
    }
  }
  return matches;
}

function replaceStaticSlots(line) {
  let output = "";
  let cursor = 0;
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") {
        index += 1;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    STATIC_SLOT_RE.lastIndex = index;
    const match = STATIC_SLOT_RE.exec(line);
    if (!match || match.index !== index) continue;
    output += line.slice(cursor, index);
    output += staticSlotName(match[1]);
    cursor = index + match[0].length;
    index = cursor - 1;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}
