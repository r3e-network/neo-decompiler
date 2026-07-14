export function inferNativePatterns(hint, label, patterns, evidence) {
  switch (hint.contract) {
    case "OracleContract":
      add(patterns, evidence, "oracle", label);
      break;
    case "Governance":
      add(patterns, evidence, "governance", label);
      break;
    case "RoleManagement":
      add(patterns, evidence, "role_management", label);
      break;
    case "PolicyContract":
      add(patterns, evidence, "policy_management", label);
      break;
    case "TokenManagement":
      add(patterns, evidence, "token_management", label);
      break;
    case "LedgerContract":
      add(patterns, evidence, "ledger", label);
      add(patterns, evidence, "blockchain_queries", label);
      break;
    case "Notary":
      add(patterns, evidence, "notary", label);
      break;
    case "Treasury":
      add(patterns, evidence, "treasury", label);
      break;
    case "CryptoLib":
      add(patterns, evidence, "cryptography", label);
      break;
    case "StdLib":
      inferStdlibPatterns(hint.canonicalMethod, label, patterns, evidence);
      break;
    case "GasToken":
    case "NeoToken":
      add(patterns, evidence, "native_token_calls", label);
      break;
    case "ContractManagement":
      add(patterns, evidence, "contract_management", label);
      if (["Deploy", "Destroy", "Update"].includes(hint.canonicalMethod)) {
        add(patterns, evidence, "contract_lifecycle", label);
        if (hint.canonicalMethod === "Update") add(patterns, evidence, "upgradeable", label);
      } else if ([
        "GetContract",
        "GetContractById",
        "GetContractHashes",
        "HasMethod",
        "IsContract",
      ].includes(hint.canonicalMethod)) {
        add(patterns, evidence, "contract_queries", label);
      }
      break;
    default:
      break;
  }
}

function inferStdlibPatterns(method, label, patterns, evidence) {
  if ([
    "Base58CheckDecode",
    "Base58CheckEncode",
    "Base58Decode",
    "Base58Encode",
    "Base64Decode",
    "Base64Encode",
    "Base64UrlDecode",
    "Base64UrlEncode",
    "Deserialize",
    "HexDecode",
    "HexEncode",
    "JsonDeserialize",
    "JsonSerialize",
    "Serialize",
  ].includes(method)) {
    add(patterns, evidence, "serialization", label);
  } else if (["Atoi", "Itoa", "StrLen", "StringSplit"].includes(method)) {
    add(patterns, evidence, "string_operations", label);
  } else if (["MemoryCompare", "MemorySearch"].includes(method)) {
    add(patterns, evidence, "memory_operations", label);
  }
}

function add(patterns, evidence, pattern, label) {
  patterns.add(pattern);
  evidence.push({
    source: "nef.method_tokens.pattern",
    value: pattern + ": " + label,
  });
}
