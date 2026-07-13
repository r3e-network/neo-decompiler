import {
  classifyPermissionContract,
  extractContractName,
  formatManifestType,
  sanitizeIdentifier,
} from "../manifest.js";
import { describeCallFlags } from "../nef.js";
import { describeMethodToken } from "../native-contracts.js";
import { upperHex } from "../util.js";
export function renderContractHeader(manifest, context = null) {
  const contractName = extractContractName(manifest);
  const lines = [`contract ${contractName} {`];
  // Contract-level metadata block (matches the Rust renderer):
  // - script hash in both byte orders for cross-explorer lookups
  // - supportedstandards / features / permissions / trusts when set
  // - extra fields like Author/Email surfaced as `// Key: Value`
  // - ABI method signatures listed as forward declarations
  // - ABI events listed as `event Name(params);`
  const scriptHash = context?.scriptHash;
  const scriptHashLE = context?.scriptHashLE;
  if (scriptHash) {
    lines.push(`    // script hash (little-endian): ${scriptHashLE}`);
    lines.push(`    // script hash (big-endian): ${scriptHash}`);
  }
  if (context?.compiler) {
    lines.push(`    // compiler: ${context.compiler}`);
  }
  if (context?.source) {
    lines.push(`    // source: ${context.source}`);
  }
  if (!manifest) {
    // Mirror the Rust header writer so the absence of an ABI surface
    // is explicit rather than silently elided. The trailing blank
    // line (separating header from body) is unconditionally added
    // below so we don't push it here — earlier this branch eagerly
    // pushed a blank line, which compounded with the method-tokens
    // header for `// manifest not provided` + blank + `// method
    // tokens declared in NEF`, while Rust runs them flush.
    lines.push(`    // manifest not provided`);
  }
  if (manifest) {
    if (manifest.supportedStandards?.length) {
      const formatted = manifest.supportedStandards.map((s) => `"${s}"`).join(", ");
      lines.push(`    supported_standards = [${formatted}];`);
    }
    // Valid Neo N3 manifests carry an empty `features` object; only a
    // malformed manifest has content here, surfaced verbatim (mirrors
    // the Rust manifest summary renderer).
    if (manifest.features && Object.keys(manifest.features).length > 0) {
      lines.push(`    features {`);
      // Rust's `serde_json::Map` is a BTreeMap (no `preserve_order`), so it
      // iterates keys in sorted order. Sort here to match — otherwise the two
      // ports emit the same keys in different order for the same manifest.
      for (const key of Object.keys(manifest.features).sort()) {
        lines.push(`        ${key} = ${JSON.stringify(manifest.features[key])};`);
      }
      lines.push(`    }`);
    }
    if (manifest.groups?.length) {
      // `groups` is the list of pubkey/signature pairs that authorise
      // signed updates of the contract. Show only the pubkey
      // (canonical short form) for a scannable summary; the signature
      // is opaque base64 and adds no human-readable value.
      lines.push(`    groups {`);
      for (const group of manifest.groups) {
        if (group?.pubkey) {
          lines.push(`        pubkey=${group.pubkey}`);
        }
      }
      lines.push(`    }`);
    }
    if (manifest.permissions?.length) {
      lines.push(`    permissions {`);
      for (const perm of manifest.permissions) {
        // Mirror Rust's `ManifestPermissionContract::describe()`:
        // wildcard verbatim, `hash:`/`group:` prefixes for official
        // string descriptors, raw JSON for malformed descriptors.
        const classified = classifyPermissionContract(perm.contract);
        const contractPart =
          classified.kind === "wildcard"
            ? `contract=${classified.value}`
            : classified.kind === "hash"
              ? `contract=hash:${classified.hash}`
              : classified.kind === "group"
                ? `contract=group:${classified.group}`
                : `contract=${JSON.stringify(classified.value)}`;
        // An absent `methods` defaults to the `*` wildcard in Neo N3 (Rust's
        // `ManifestPermissionMethods` derives `Default = Wildcard("*")` via
        // `#[serde(default)]`). Without this guard the final branch would
        // render the literal string `methods=undefined`.
        const methodsPart =
          perm.methods === undefined || perm.methods === null
            ? "methods=*"
            : typeof perm.methods === "string"
              ? `methods=${perm.methods}`
              : Array.isArray(perm.methods)
                ? `methods=[${perm.methods.map((m) => `"${m}"`).join(", ")}]`
                : `methods=${JSON.stringify(perm.methods)}`;
        lines.push(`        ${contractPart} ${methodsPart}`);
      }
      lines.push(`    }`);
    }
    if (manifest.trusts !== null && manifest.trusts !== undefined) {
      const formatted = formatManifestTrusts(manifest.trusts);
      if (formatted !== null) {
        lines.push(`    trusts = ${formatted};`);
      }
    }
    if (manifest.extra && typeof manifest.extra === "object" && !Array.isArray(manifest.extra)) {
      // Rust iterates `extra` (a sorted `serde_json::Map`/BTreeMap) in key
      // order; sort here so the metadata comment lines match. `extra`
      // (Author/Email/Version/…) is common in valid manifests and tooling does
      // not guarantee alphabetical key order, so this diverges on real input.
      for (const key of Object.keys(manifest.extra).sort()) {
        const rendered = renderExtraScalar(manifest.extra[key]);
        if (rendered !== null) {
          lines.push(`    // ${key}: ${rendered}`);
        }
      }
    }
    if (manifest.abi?.methods?.length) {
      lines.push(`    // ABI methods`);
      for (const method of manifest.abi.methods) {
        const params = method.parameters
          ?.map((p) => `${sanitizeIdentifier(p.name)}: ${formatManifestType(p.kind)}`)
          .join(", ") ?? "";
        // Always show `-> type`, including `-> void`, in the ABI summary
        // so the manifest contract surface is fully explicit. The lifted
        // method body still omits `-> void` for idiomatic readability.
        const returnType = formatManifestType(method.returnType ?? "Void");
        // Build the trailing meta-comment with the same shape as the
        // Rust port: when the manifest method name has chars that
        // sanitise away (e.g. `-`), surface a `manifest "Original"`
        // entry so the original identifier is recoverable. Then
        // `safe` (if `safe: true`) and `offset N` join with `, `.
        const sanitisedName = sanitizeIdentifier(method.name);
        const meta = [];
        if (sanitisedName !== method.name) {
          meta.push(`manifest ${JSON.stringify(method.name)}`);
        }
        if (method.safe) {
          meta.push("safe");
        }
        if (typeof method.offset === "number") {
          meta.push(`offset ${method.offset}`);
        }
        const metaComment = meta.length > 0 ? ` // ${meta.join(", ")}` : "";
        lines.push(`    fn ${sanitisedName}(${params}) -> ${returnType};${metaComment}`);
      }
    }
    if (manifest.abi?.events?.length) {
      lines.push(`    // ABI events`);
      for (const event of manifest.abi.events) {
        const params = event.parameters
          ?.map((p) => `${sanitizeIdentifier(p.name)}: ${formatManifestType(p.kind)}`)
          .join(", ") ?? "";
        const sanitised = sanitizeIdentifier(event.name);
        // Mirror the Rust manifest summary: when the sanitised
        // identifier differs from the raw manifest name, append a
        // `// manifest "Original"` annotation so the original
        // identifier is recoverable from the lifted source.
        const note = sanitised !== event.name ? ` // manifest ${JSON.stringify(event.name)}` : "";
        lines.push(`    event ${sanitised}(${params});${note}`);
      }
    }
  }
  // Method tokens declared in the NEF — surface them whether or not a
  // manifest was supplied, mirroring the Rust contract header.
  const methodTokens = context?.methodTokens ?? [];
  if (methodTokens.length > 0) {
    lines.push(`    // method tokens declared in NEF`);
    for (const token of methodTokens) {
      const hint = describeMethodToken(token.hash, token.method);
      const contractNote = hint ? ` (${hint.formattedLabel(token.method)})` : "";
      const flagsHex = token.callFlags.toString(16).padStart(2, "0").toUpperCase();
      lines.push(
        `    // ${token.method}${contractNote} hash=${upperHex(token.hash)} ` +
          `params=${token.parametersCount} returns=${token.hasReturnValue} ` +
          `flags=0x${flagsHex} (${describeCallFlags(token.callFlags)})`,
      );
      if (hint && !hint.hasExactMethod()) {
        lines.push(
          `    // warning: native contract ${hint.contract} does not expose method ${token.method}`,
        );
      }
    }
  }
  // Single trailing blank line separating header from method bodies.
  // Mirrors `writeln!(output)` at the end of Rust's
  // `write_contract_header` — emitted unconditionally regardless of
  // whether a manifest or method tokens were rendered.
  lines.push("");
  return lines;
}
function renderExtraScalar(value) {
  if (typeof value === "string") return value;
  if (typeof value === "boolean") return value.toString();
  if (typeof value === "number" && Number.isFinite(value)) return value.toString();
  if (typeof value === "bigint") return value.toString();
  return null;
}

function formatManifestTrusts(trusts) {
  if (trusts === "*") {
    return "*";
  }
  if (Array.isArray(trusts)) {
    if (trusts.length === 0) {
      // An explicit empty `trusts: []` (trust nobody) renders as `[]`, matching
      // the Rust port's ManifestTrusts::describe (`trusts = [];`).
      return "[]";
    }
    if (trusts.every((entry) => typeof entry === "string")) {
      return `[${trusts.map((entry) => `"${entry}"`).join(", ")}]`;
    }
    return JSON.stringify(trusts);
  }
  if (trusts && typeof trusts === "object") {
    const structured = formatStructuredTrusts(trusts);
    if (structured !== null) {
      return structured;
    }
  }
  return JSON.stringify(trusts);
}

function formatStructuredTrusts(object) {
  const allowedKeys = new Set(["hashes", "groups"]);
  for (const key of Object.keys(object)) {
    if (!allowedKeys.has(key)) {
      return null;
    }
  }
  const hashes = parseTypedTrustEntries(object.hashes, "hash");
  if (hashes === null) {
    return null;
  }
  const groups = parseTypedTrustEntries(object.groups, "group");
  if (groups === null) {
    return null;
  }
  return `[${[...hashes, ...groups].join(", ")}]`;
}

function parseTypedTrustEntries(value, prefix) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    return null;
  }
  const entries = [];
  for (const entry of value) {
    if (typeof entry !== "string") {
      return null;
    }
    entries.push(`${prefix}:${entry}`);
  }
  return entries;
}
