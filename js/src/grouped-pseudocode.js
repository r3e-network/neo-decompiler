import { renderPseudocode } from "./pseudocode.js";
import { sanitizeIdentifier } from "./manifest.js";

export function renderGroupedPseudocode(groups, manifest) {
  const contractName = manifest ? sanitizeIdentifier(manifest.name) : "Contract";
  let output = `contract ${contractName} {\n`;

  for (const group of groups) {
    output += `    fn ${group.name}() {\n`;
    const body = renderPseudocode(group.instructions)
      .trimEnd()
      .split("\n")
      .filter(Boolean);
    for (const line of body) {
      output += `        ${line}\n`;
    }
    output += "    }\n";
  }

  output += "}\n";
  return output;
}
