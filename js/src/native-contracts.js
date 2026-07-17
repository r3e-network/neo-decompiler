// Mirror of Rust `src/native_contracts*.rs`. Provides a lookup from a
// 20-byte (UInt160) script hash + method name to a `NativeMethodHint`
// the renderer uses to attach a friendly `(Contract::Method)` annotation
// to method tokens. Keep the native-contract table in sync with
// `src/native_contracts_generated.rs` — both are produced by
// `tools/scrape_native_contracts.py`.

const NATIVE_CONTRACTS = [
  {
    name: "CryptoLib",
    scriptHash: new Uint8Array([
      0x1B, 0xF5, 0x75, 0xAB, 0x11, 0x89, 0x68, 0x84, 0x13, 0x61, 0x0A, 0x35,
      0xA1, 0x28, 0x86, 0xCD, 0xE0, 0xB6, 0x6C, 0x72,
    ]),
    methods: [
      "Bls12381Add", "Bls12381Deserialize", "Bls12381Equal", "Bls12381Mul",
      "Bls12381Pairing", "Bls12381Serialize", "Keccak256", "Murmur32",
      "Sha256", "VerifyWithECDsa", "VerifyWithEd25519", "recoverSecp256K1",
      "ripemd160", "verifyWithECDsa",
    ],
  },
  {
    name: "Notary",
    scriptHash: new Uint8Array([
      0x3B, 0xEC, 0x35, 0x31, 0x11, 0x9B, 0xBA, 0xD7, 0x6D, 0xD0, 0x44, 0x92,
      0x0B, 0x0D, 0xE6, 0xC3, 0x19, 0x4F, 0xE1, 0xC1,
    ]),
    methods: [
      "BalanceOf", "ExpirationOf", "GetMaxNotValidBeforeDelta",
      "LockDepositUntil", "OnNEP17Payment", "SetMaxNotValidBeforeDelta",
      "Verify", "Withdraw", "_OnPayment",
    ],
  },
  {
    name: "OracleContract",
    scriptHash: new Uint8Array([
      0x58, 0x87, 0x17, 0x11, 0x7E, 0x0A, 0xA8, 0x10, 0x72, 0xAF, 0xAB, 0x71,
      0xD2, 0xDD, 0x89, 0xFE, 0x7C, 0x4B, 0x92, 0xFE,
    ]),
    methods: ["Finish", "GetPrice", "Request", "SetPrice", "Verify"],
  },
  {
    name: "Governance",
    scriptHash: new Uint8Array([
      0x67, 0xCA, 0x70, 0x35, 0x06, 0x63, 0xBF, 0x25, 0x8C, 0xA5, 0x13, 0x04,
      0x94, 0x67, 0xC6, 0x05, 0x9D, 0x15, 0xE7, 0x4C,
    ]),
    methods: [
      "GetAllCandidates", "GetCandidateVote", "GetCandidates",
      "GetCommittee", "GetCommitteeAddress", "GetGasPerBlock",
      "GetNextBlockValidators", "GetRegisterPrice", "GetVoteTarget",
      "RegisterCandidate", "SetGasPerBlock", "SetRegisterPrice",
      "UnclaimedGas", "UnregisterCandidate", "Vote",
    ],
  },
  {
    name: "PolicyContract",
    scriptHash: new Uint8Array([
      0x7B, 0xC6, 0x81, 0xC0, 0xA1, 0xF7, 0x1D, 0x54, 0x34, 0x57, 0xB6, 0x8B,
      0xBA, 0x8D, 0x5F, 0x9F, 0xDD, 0x4E, 0x5E, 0xCC,
    ]),
    methods: [
      "BlockAccount", "GetAttributeFee", "GetBlockedAccounts",
      "GetExecFeeFactor", "GetExecPicoFeeFactor", "GetFeePerByte",
      "GetMaxTraceableBlocks", "GetMaxValidUntilBlockIncrement",
      "GetMillisecondsPerBlock", "GetStoragePrice",
      "GetWhitelistFeeContracts", "IsBlocked", "RecoverFund",
      "RemoveWhitelistFeeContract", "SetAttributeFee", "SetExecFeeFactor",
      "SetFeePerByte", "SetMaxTraceableBlocks",
      "SetMaxValidUntilBlockIncrement", "SetMillisecondsPerBlock",
      "SetStoragePrice", "SetWhitelistFeeContract", "UnblockAccount",
      "blockAccount", "getAttributeFee", "setAttributeFee",
    ],
  },
  {
    name: "TokenManagement",
    scriptHash: new Uint8Array([
      0x9F, 0x04, 0x0E, 0xA4, 0xA8, 0x44, 0x8F, 0x01, 0x5A, 0xF6, 0x45, 0x65,
      0x9B, 0x0F, 0xB2, 0xAE, 0x7D, 0xC5, 0x00, 0xAE,
    ]),
    methods: ["BalanceOf", "GetAssetsOfOwner", "GetTokenInfo"],
  },
  {
    name: "LedgerContract",
    scriptHash: new Uint8Array([
      0xBE, 0xF2, 0x04, 0x31, 0x40, 0x36, 0x2A, 0x77, 0xC1, 0x50, 0x99, 0xC7,
      0xE6, 0x4C, 0x12, 0xF7, 0x00, 0xB6, 0x65, 0xDA,
    ]),
    methods: [
      "CurrentHash", "CurrentIndex", "GetBlock", "GetTransactionFromBlock",
      "GetTransactionHeight", "GetTransactionSigners",
      "GetTransactionVMState", "getTransaction",
    ],
  },
  {
    name: "StdLib",
    scriptHash: new Uint8Array([
      0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A,
      0x79, 0xE1, 0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
    ]),
    methods: [
      "Atoi", "Base58CheckDecode", "Base58CheckEncode", "Base58Decode",
      "Base58Encode", "Base64Decode", "Base64Encode", "Base64UrlDecode",
      "Base64UrlEncode", "Deserialize", "HexDecode", "HexEncode", "Itoa",
      "JsonDeserialize", "JsonSerialize", "MemoryCompare", "MemorySearch",
      "Serialize", "StrLen", "StringSplit",
    ],
  },
  {
    name: "Treasury",
    scriptHash: new Uint8Array([
      0xC1, 0x3A, 0x56, 0xC9, 0x83, 0x53, 0xA7, 0xEA, 0x6A, 0x32, 0x4D, 0x9A,
      0x83, 0x5D, 0x1B, 0x5B, 0xF2, 0x26, 0x63, 0x15,
    ]),
    methods: ["OnNEP11Payment", "OnNEP17Payment", "Verify"],
  },
  {
    name: "GasToken",
    scriptHash: new Uint8Array([
      0xCF, 0x76, 0xE2, 0x8B, 0xD0, 0x06, 0x2C, 0x4A, 0x47, 0x8E, 0xE3, 0x55,
      0x61, 0x01, 0x13, 0x19, 0xF3, 0xCF, 0xA4, 0xD2,
    ]),
    methods: [
      "BalanceOf", "Decimals", "Symbol", "TotalSupply", "Transfer",
    ],
  },
  {
    name: "RoleManagement",
    scriptHash: new Uint8Array([
      0xE2, 0x95, 0xE3, 0x91, 0x54, 0x4C, 0x17, 0x8A, 0xD9, 0x4F, 0x03, 0xEC,
      0x4D, 0xCD, 0xFF, 0x78, 0x53, 0x4E, 0xCF, 0x49,
    ]),
    methods: ["DesignateAsRole", "GetDesignatedByRole"],
  },
  {
    name: "NeoToken",
    scriptHash: new Uint8Array([
      0xF5, 0x63, 0xEA, 0x40, 0xBC, 0x28, 0x3D, 0x4D, 0x0E, 0x05, 0xC4, 0x8E,
      0xA3, 0x05, 0xB3, 0xF2, 0xA0, 0x73, 0x40, 0xEF,
    ]),
    methods: [
      "BalanceOf", "Decimals", "GetAccountState", "GetAllCandidates",
      "GetCandidateVote", "GetCandidates", "GetCommittee",
      "GetCommitteeAddress", "GetGasPerBlock", "GetNextBlockValidators",
      "GetRegisterPrice", "OnNEP17Payment", "RegisterCandidate",
      "SetGasPerBlock", "SetRegisterPrice", "Symbol", "TotalSupply",
      "Transfer", "UnclaimedGas", "UnregisterCandidate", "Vote",
    ],
  },
  {
    name: "ContractManagement",
    scriptHash: new Uint8Array([
      0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4, 0x97,
      0xDD, 0xAD, 0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
    ]),
    methods: [
      "Deploy", "Destroy", "GetContract", "GetContractById",
      "GetContractHashes", "GetMinimumDeploymentFee", "HasMethod",
      "IsContract", "SetMinimumDeploymentFee", "Update",
    ],
  },
];

const FRAMEWORK_METHOD_NAMES = new Map([
  ["CryptoLib:recoverSecp256K1", "RecoverSecp256K1"],
  ["CryptoLib:ripemd160", "Ripemd160"],
  ["CryptoLib:verifyWithECDsa", "VerifyWithECDsa"],
  ["LedgerContract:getTransaction", "GetTransaction"],
  ["NeoToken:UnregisterCandidate", "UnRegisterCandidate"],
  ["PolicyContract:getAttributeFee", "GetAttributeFee"],
]);

function bytesEqual(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function lookup(hash) {
  for (const contract of NATIVE_CONTRACTS) {
    if (bytesEqual(contract.scriptHash, hash)) {
      return contract;
    }
  }
  return null;
}

/**
 * Return native contract guidance for the supplied method token, or null
 * if the hash does not match any known native contract. Mirrors the Rust
 * `describe_method_token` helper.
 *
 * The returned hint exposes:
 *  - `contract` — canonical contract name (e.g. `"StdLib"`)
 *  - `formattedLabel(provided)` — string of the form
 *    `Contract::Method` when `provided` matches one of the contract's
 *    advertised methods (case-sensitive, then case-insensitive), or
 *    `Contract::<unknown provided>` when it does not.
 */
export function describeMethodToken(hash, method) {
  const contract = lookup(hash);
  if (!contract) return null;
  const exact = contract.methods.find((candidate) => candidate === method);
  const ciMatch = exact
    ?? contract.methods.find(
      (candidate) => candidate.toLowerCase() === method.toLowerCase(),
    );
  return {
    contract: contract.name,
    canonicalMethod: ciMatch ?? null,
    formattedLabel(provided) {
      if (ciMatch) {
        return `${contract.name}::${ciMatch}`;
      }
      return `${contract.name}::<unknown ${provided}>`;
    },
    hasExactMethod() {
      return ciMatch !== undefined && ciMatch !== null;
    },
  };
}

/**
 * Return the spelling used by the Neo C# framework for a catalogued native
 * method. VM method names intentionally preserve protocol casing, while a
 * few framework declarations use CLR-style names.
 */
export function frameworkMethodName(contract, method) {
  return FRAMEWORK_METHOD_NAMES.get(`${contract}:${method}`) ?? method;
}

/**
 * Stable C# return type for a catalogued native method, mirroring Rust
 * `native_method_types::lookup` once the contract/method identity is known.
 *
 * High-level source already names the native contract (`StdLib::Itoa`), so a
 * hash is not required for this surface. Unknown or void methods return null
 * so callers stay dynamically typed instead of inventing a return value.
 */
export function nativeMethodReturnType(contract, method) {
  if (typeof contract !== "string" || typeof method !== "string") return null;
  const canonical = canonicalNativeMethod(contract, method);
  if (!canonical) return null;
  return NATIVE_METHOD_RETURN_TYPES.get(`${contract}:${canonical}`) ?? null;
}

function canonicalNativeMethod(contract, method) {
  const entry = NATIVE_CONTRACTS.find((candidate) => candidate.name === contract);
  if (!entry) return null;
  const exact = entry.methods.find((candidate) => candidate === method);
  if (exact) return exact;
  const ciMatch = entry.methods.find(
    (candidate) => candidate.toLowerCase() === method.toLowerCase(),
  );
  if (ciMatch) return ciMatch;
  // Accept framework spellings (RecoverSecp256K1) that differ from the
  // catalogued VM names (recoverSecp256K1).
  for (const [key, frameworkName] of FRAMEWORK_METHOD_NAMES) {
    if (frameworkName !== method) continue;
    const [mappedContract, mappedMethod] = key.split(":");
    if (mappedContract !== contract) continue;
    if (entry.methods.includes(mappedMethod)) return mappedMethod;
  }
  return null;
}

// Keep these C# spellings aligned with src/decompiler/native_method_types.rs.
const NATIVE_METHOD_RETURN_TYPES = new Map([
  ["ContractManagement:Deploy", "Contract"],
  ["ContractManagement:GetContract", "Contract"],
  ["ContractManagement:GetContractById", "Contract"],
  ["ContractManagement:GetContractHashes", "Iterator<(int, UInt160)>"],
  ["ContractManagement:HasMethod", "bool"],
  ["ContractManagement:IsContract", "bool"],
  ["ContractManagement:GetMinimumDeploymentFee", "long"],

  ["CryptoLib:Bls12381Deserialize", "object"],
  ["CryptoLib:Bls12381Add", "object"],
  ["CryptoLib:Bls12381Mul", "object"],
  ["CryptoLib:Bls12381Pairing", "object"],
  ["CryptoLib:Bls12381Equal", "bool"],
  ["CryptoLib:recoverSecp256K1", "ByteString"],
  ["CryptoLib:Keccak256", "ByteString"],
  ["CryptoLib:Murmur32", "ByteString"],
  ["CryptoLib:Sha256", "ByteString"],
  ["CryptoLib:ripemd160", "ByteString"],
  ["CryptoLib:VerifyWithECDsa", "bool"],
  ["CryptoLib:VerifyWithEd25519", "bool"],
  ["CryptoLib:verifyWithECDsa", "bool"],
  ["CryptoLib:Bls12381Serialize", "byte[]"],

  ["LedgerContract:CurrentHash", "UInt256"],
  ["LedgerContract:CurrentIndex", "uint"],
  ["LedgerContract:GetBlock", "Block"],
  ["LedgerContract:getTransaction", "Transaction"],
  ["LedgerContract:GetTransactionFromBlock", "Transaction"],
  ["LedgerContract:GetTransactionHeight", "int"],
  ["LedgerContract:GetTransactionVMState", "VMState"],
  ["LedgerContract:GetTransactionSigners", "Signer[]"],

  ["GasToken:Symbol", "string"],
  ["NeoToken:Symbol", "string"],
  ["GasToken:Decimals", "byte"],
  ["NeoToken:Decimals", "byte"],
  ["GasToken:BalanceOf", "BigInteger"],
  ["NeoToken:BalanceOf", "BigInteger"],
  ["GasToken:GetGasPerBlock", "BigInteger"],
  ["NeoToken:GetGasPerBlock", "BigInteger"],
  ["GasToken:TotalSupply", "BigInteger"],
  ["NeoToken:TotalSupply", "BigInteger"],
  ["GasToken:UnclaimedGas", "BigInteger"],
  ["NeoToken:UnclaimedGas", "BigInteger"],
  ["GasToken:Transfer", "bool"],
  ["NeoToken:Transfer", "bool"],
  ["NeoToken:GetRegisterPrice", "long"],
  ["NeoToken:RegisterCandidate", "bool"],
  ["NeoToken:UnregisterCandidate", "bool"],
  ["NeoToken:Vote", "bool"],
  ["NeoToken:GetCandidates", "(ECPoint, BigInteger)[]"],
  ["NeoToken:GetAllCandidates", "Iterator<(ECPoint, BigInteger)>"],
  ["NeoToken:GetCandidateVote", "BigInteger"],
  ["NeoToken:GetCommittee", "ECPoint[]"],
  ["NeoToken:GetNextBlockValidators", "ECPoint[]"],
  ["NeoToken:GetCommitteeAddress", "UInt160"],
  ["NeoToken:GetAccountState", "NeoAccountState"],

  ["Notary:BalanceOf", "BigInteger"],
  ["Notary:ExpirationOf", "uint"],
  ["Notary:GetMaxNotValidBeforeDelta", "uint"],
  ["Notary:LockDepositUntil", "bool"],
  ["Notary:Verify", "bool"],
  ["Notary:Withdraw", "bool"],

  ["OracleContract:GetPrice", "long"],

  ["PolicyContract:GetAttributeFee", "uint"],
  ["PolicyContract:getAttributeFee", "uint"],
  ["PolicyContract:GetExecFeeFactor", "uint"],
  ["PolicyContract:GetStoragePrice", "uint"],
  ["PolicyContract:GetExecPicoFeeFactor", "BigInteger"],
  ["PolicyContract:GetFeePerByte", "long"],
  ["PolicyContract:IsBlocked", "bool"],
  ["PolicyContract:GetBlockedAccounts", "Iterator"],
  ["PolicyContract:GetWhitelistFeeContracts", "Iterator"],

  ["Treasury:Verify", "bool"],
  ["RoleManagement:GetDesignatedByRole", "ECPoint[]"],

  ["StdLib:Atoi", "BigInteger"],
  ["StdLib:Deserialize", "object"],
  ["StdLib:JsonDeserialize", "object"],
  ["StdLib:Itoa", "string"],
  ["StdLib:Base64Encode", "string"],
  ["StdLib:Base64UrlEncode", "string"],
  ["StdLib:Base58Encode", "string"],
  ["StdLib:Base58CheckEncode", "string"],
  ["StdLib:HexEncode", "string"],
  ["StdLib:Base64Decode", "ByteString"],
  ["StdLib:Base64UrlDecode", "ByteString"],
  ["StdLib:Base58Decode", "ByteString"],
  ["StdLib:Base58CheckDecode", "ByteString"],
  ["StdLib:HexDecode", "ByteString"],
  ["StdLib:Serialize", "ByteString"],
  ["StdLib:JsonSerialize", "string"],
  ["StdLib:MemoryCompare", "BigInteger"],
  ["StdLib:MemorySearch", "BigInteger"],
  ["StdLib:StrLen", "BigInteger"],
  ["StdLib:StringSplit", "object[]"],
]);
