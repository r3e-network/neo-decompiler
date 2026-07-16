// Labels are method-scoped in C#. A partially recovered VM branch can still
// leave a transfer to a label that was not emitted (or belonged to another
// method slice). Preserve valid transfers and turn only the unresolvable ones
// into explicit comments so the generated contract remains parseable.
export function rewriteUnresolvedGotos(line, labels = new Set()) {
  const hasGoto = /\bgoto\s+(label_0x[0-9A-Fa-f]+);/i.test(line);
  if (!hasGoto) return line;
  const visibleLabels = labels.labels ?? labels;
  const labelDepths = labels.labelDepths ?? new Map();
  const finallyLabels = labels.finallyLabels ?? new Set();
  const currentDepth = labels.depth ?? Number.POSITIVE_INFINITY;
  let unresolved = false;
  const rewritten = line.replace(/\bgoto\s+(label_0x[0-9A-Fa-f]+);/gi, (full, label) => {
    const normalized = label.toLowerCase();
    if (
      visibleLabels.has(normalized) &&
      !finallyLabels.has(normalized) &&
      (labelDepths.get(normalized) ?? currentDepth) <= currentDepth
    ) {
      return full;
    }
    unresolved = true;
    return `/* unresolved control transfer: goto ${label}; */`;
  });
  if (!unresolved) return rewritten;
  return /^\s*goto\s+/.test(line)
    ? `${line.match(/^\s*/)?.[0] ?? ""}// unresolved control transfer: ${line.trim()}`
    : rewritten;
}

export function labelsVisibleInMethod(lines, depths) {
  const labelsByLine = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    let end = start + 1;
    while (end < lines.length) {
      if (depths[end] === methodDepth + 1 && /^\s*}\s*$/.test(lines[end])) break;
      end += 1;
    }
    const labels = new Set();
    const labelDepths = new Map();
    const finallyLabels = new Set();
    for (let index = start + 1; index < end; index += 1) {
      const match = lines[index].trim().match(/^(label_0x[0-9A-Fa-f]+):/i);
      if (match) {
        const normalized = match[1].toLowerCase();
        labels.add(normalized);
        labelDepths.set(normalized, depths[index]);
      }
    }
    for (let index = start + 1; index < end; index += 1) {
      if (!/\bfinally\s*\{\s*$/.test(lines[index])) continue;
      const finallyDepth = depths[index];
      for (let cursor = index + 1; cursor < end; cursor += 1) {
        if (depths[cursor] === finallyDepth + 1 && /^\s*}\s*$/.test(lines[cursor])) break;
        const match = lines[cursor].trim().match(/^(label_0x[0-9A-Fa-f]+):/i);
        if (match) finallyLabels.add(match[1].toLowerCase());
      }
    }
    for (let index = start; index <= end; index += 1) {
      labelsByLine.set(index, { labels, labelDepths, finallyLabels, depth: depths[index] });
    }
    start = end;
  }
  return labelsByLine;
}
