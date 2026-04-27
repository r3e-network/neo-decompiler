import { renderPseudocode } from "./pseudocode.js";
import { extractContractName } from "./manifest.js";

export function renderGroupedPseudocode(groups, manifest) {
  // Use the shared `extractContractName` so the contract-name fallback
  // (`NeoContract` for manifest-less or empty-name inputs) matches the
  // high-level renderer and Rust's `extract_contract_name`. Earlier
  // this fell back to `Contract`, diverging from the rest of the
  // codebase even though no live caller currently hits the null-
  // manifest branch (both call sites in index.js parse a manifest
  // before invoking us).
  const contractName = extractContractName(manifest);
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
