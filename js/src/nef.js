import { NefParseError } from "./errors.js";
import {
  asUint8Array,
  computeChecksum,
  computeScriptHash,
  readU16LE,
  readU32LE,
  readU64LE,
  upperHex,
  upperHexReversed,
} from "./util.js";

const MAX_NEF_FILE_SIZE = 0x10_0000;
const FIXED_HEADER_SIZE = 68;
const CHECKSUM_SIZE = 4;
const MAGIC = "NEF3";
// 'NEF3' interpreted as a little-endian uint32, per the reference
// `NefFile.Magic` definition.
const MAGIC_U32 = 0x3346454e;
const MAX_SOURCE_LEN = 256;
// Reference: `NefFile.Deserialize` reads tokens with
// `reader.ReadSerializableArray<MethodToken>(128)`.
const MAX_METHOD_TOKENS = 128;
const MAX_SCRIPT_LEN = 0x8_0000;
// Reference: `MethodToken.Deserialize` reads the method name with
// `reader.ReadVarString(32)`.
const MAX_METHOD_NAME_LEN = 32;
const CALL_FLAGS_ALLOWED_MASK = 0x0f;

const textDecoder = new TextDecoder("utf-8", { fatal: true });

export function parseNef(input) {
  const bytes = asUint8Array(input);
  if (bytes.byteLength > MAX_NEF_FILE_SIZE) {
    throw new NefParseError(
      `file size ${bytes.byteLength} exceeds maximum (${MAX_NEF_FILE_SIZE} bytes)`,
      { code: "FileTooLarge", size: bytes.byteLength, max: MAX_NEF_FILE_SIZE },
    );
  }
  if (bytes.byteLength < FIXED_HEADER_SIZE + CHECKSUM_SIZE) {
    throw new NefParseError("file too short to contain a NEF header", {
      code: "TooShort",
    });
  }

  let offset = 0;
  // The spec treats magic as a uint32 (0x3346454E); compare numerically
  // rather than decoding text so invalid UTF-8 in the first four bytes
  // still surfaces as InvalidMagic instead of a raw TypeError.
  const magicBytes = slice(bytes, offset, 4);
  if (readU32LE(magicBytes, 0) !== MAGIC_U32) {
    const actualHex = upperHex(magicBytes);
    throw new NefParseError(
      `invalid magic bytes: expected ${JSON.stringify(MAGIC)}, got 0x${actualHex}`,
      { code: "InvalidMagic", expected: MAGIC, actual: actualHex },
    );
  }
  offset += 4;

  const compilerBytes = slice(bytes, offset, 64);
  let compiler;
  try {
    const nullIdx = compilerBytes.indexOf(0);
    const trimmed = nullIdx === -1 ? compilerBytes : compilerBytes.subarray(0, nullIdx);
    compiler = textDecoder.decode(trimmed);
  } catch {
    throw new NefParseError("compiler field is not valid UTF-8", {
      code: "InvalidCompiler",
    });
  }
  offset += 64;

  const sourceResult = readVarString(bytes, offset, MAX_SOURCE_LEN);
  const source = sourceResult.value;
  offset += sourceResult.consumed;

  const reservedByte = bytes[offset];
  if (reservedByte === undefined) {
    throw unexpectedEof(offset);
  }
  if (reservedByte !== 0) {
    throw new NefParseError(
      `reserved byte at offset ${offset} must be zero (found 0x${reservedByte.toString(16).padStart(2, "0").toUpperCase()})`,
      { code: "ReservedByteNonZero", offset, value: reservedByte },
    );
  }
  offset += 1;

  const tokensResult = parseMethodTokens(bytes, offset);
  const methodTokens = tokensResult.tokens;
  offset += tokensResult.consumed;

  const reservedWordBytes = slice(bytes, offset, 2);
  const reservedWord = readU16LE(reservedWordBytes, 0);
  if (reservedWord !== 0) {
    throw new NefParseError(
      `reserved word at offset ${offset} must be zero (found 0x${reservedWord.toString(16).padStart(4, "0").toUpperCase()})`,
      { code: "ReservedWordNonZero", offset, value: reservedWord },
    );
  }
  offset += 2;

  const scriptResult = readVarBytes(bytes, offset, MAX_SCRIPT_LEN);
  const script = scriptResult.value;
  if (script.byteLength === 0) {
    throw new NefParseError("script section cannot be empty", {
      code: "EmptyScript",
    });
  }
  offset += scriptResult.consumed;

  const checksumStart = offset;
  const checksumBytes = slice(bytes, checksumStart, 4);
  const checksum = readU32LE(checksumBytes, 0);
  const calculated = readU32LE(computeChecksum(bytes.subarray(0, checksumStart)), 0);
  if (checksum !== calculated) {
    throw new NefParseError(
      `checksum mismatch: expected 0x${checksum.toString(16).padStart(8, "0")}, calculated 0x${calculated.toString(16).padStart(8, "0")}`,
      { code: "ChecksumMismatch", expected: checksum, calculated },
    );
  }
  if (checksumStart + CHECKSUM_SIZE !== bytes.byteLength) {
    throw new NefParseError(
      `unexpected trailing data after checksum (extra ${bytes.byteLength - (checksumStart + CHECKSUM_SIZE)} bytes)`,
      {
        code: "TrailingData",
        extra: bytes.byteLength - (checksumStart + CHECKSUM_SIZE),
      },
    );
  }

  const scriptHashBytes = computeScriptHash(script);
  return {
    header: {
      magic: MAGIC,
      compiler,
      source,
    },
    methodTokens,
    script,
    checksum,
    // The raw RIPEMD160(SHA256(..)) digest IS Neo's internal little-endian
    // UInt160 order; the explorer/display ("big-endian", 0x-prefixed) form
    // is that digest reversed. `scriptHashLE` is therefore the raw digest
    // and `scriptHash` (the canonical display form) is the reversal.
    scriptHash: upperHexReversed(scriptHashBytes),
    scriptHashLE: upperHex(scriptHashBytes),
  };
}

/**
 * Return a `|`-joined human-readable description of the call flags set on
 * the supplied bitmask, matching the Rust `describe_call_flags` helper.
 * Returns `"None"` for the zero mask so the caller can render the flags
 * in a single uniform shape.
 */
export function describeCallFlags(flags) {
  if (flags === 0) {
    return "None";
  }
  const labels = [];
  if ((flags & 0x01) !== 0) labels.push("ReadStates");
  if ((flags & 0x02) !== 0) labels.push("WriteStates");
  if ((flags & 0x04) !== 0) labels.push("AllowCall");
  if ((flags & 0x08) !== 0) labels.push("AllowNotify");
  return labels.join("|");
}

function parseMethodTokens(bytes, startOffset) {
  let offset = startOffset;
  const countResult = readVarInt(bytes, offset);
  const count = countResult.value;
  offset += countResult.consumed;

  if (count > MAX_METHOD_TOKENS) {
    throw new NefParseError(
      `method token count exceeds maximum (${count} > ${MAX_METHOD_TOKENS})`,
      { code: "TooManyMethodTokens", count, max: MAX_METHOD_TOKENS },
    );
  }

  const tokens = [];
  for (let index = 0; index < count; index += 1) {
    const hash = slice(bytes, offset, 20);
    offset += 20;

    const methodLenResult = readVarInt(bytes, offset);
    const methodLen = methodLenResult.value;
    offset += methodLenResult.consumed;
    if (methodLen > MAX_METHOD_NAME_LEN) {
      throw new NefParseError(`invalid method token at index ${index}`, {
        code: "InvalidMethodToken",
        index,
      });
    }

    const methodBytes = slice(bytes, offset, methodLen);
    let method;
    try {
      method = textDecoder.decode(methodBytes);
    } catch {
      throw new NefParseError(`invalid method token at index ${index}`, {
        code: "InvalidMethodToken",
        index,
      });
    }
    if (method.startsWith("_")) {
      throw new NefParseError(`method token name ${JSON.stringify(method)} is not permitted`, {
        code: "MethodNameInvalid",
        name: method,
      });
    }
    offset += methodLen;

    const paramsBytes = slice(bytes, offset, 2);
    const parametersCount = readU16LE(paramsBytes, 0);
    offset += 2;

    const hasReturnValueByte = bytes[offset];
    if (hasReturnValueByte === undefined) {
      throw unexpectedEof(offset);
    }
    if (hasReturnValueByte !== 0 && hasReturnValueByte !== 1) {
      throw new NefParseError(`invalid method token at index ${index}`, {
        code: "InvalidMethodToken",
        index,
      });
    }
    const hasReturnValue = hasReturnValueByte === 1;
    offset += 1;

    const callFlags = bytes[offset];
    if (callFlags === undefined) {
      throw unexpectedEof(offset);
    }
    if ((callFlags & ~CALL_FLAGS_ALLOWED_MASK) !== 0) {
      throw new NefParseError(
        `method token call flags 0x${callFlags.toString(16).padStart(2, "0").toUpperCase()} contain unsupported bits (allowed mask 0x${CALL_FLAGS_ALLOWED_MASK.toString(16).padStart(2, "0").toUpperCase()})`,
        { code: "CallFlagsInvalid", flags: callFlags, allowed: CALL_FLAGS_ALLOWED_MASK },
      );
    }
    offset += 1;

    tokens.push({
      hash: new Uint8Array(hash),
      method,
      parametersCount,
      hasReturnValue,
      callFlags,
    });
  }

  return { tokens, consumed: offset - startOffset };
}

function readVarInt(bytes, offset) {
  const first = bytes[offset];
  if (first === undefined) {
    throw unexpectedEof(offset);
  }

  let value;
  let consumed;
  if (first <= 0xfc) {
    value = first;
    consumed = 1;
  } else if (first === 0xfd) {
    value = readU16LE(slice(bytes, offset + 1, 2), 0);
    consumed = 3;
  } else if (first === 0xfe) {
    value = readU32LE(slice(bytes, offset + 1, 4), 0);
    consumed = 5;
  } else {
    const wide = readU64LE(slice(bytes, offset + 1, 8), 0);
    if (wide > BigInt(0xffffffff)) {
      throw new NefParseError(`varint exceeds supported range at offset ${offset}`, {
        code: "IntegerOverflow",
        offset,
      });
    }
    value = Number(wide);
    consumed = 9;
  }

  // Matches the reference `MemoryReader.ReadVarInt`: the prefix selects
  // the width and only the maximum value is validated. Non-canonical
  // encodings (e.g. `FD 05 00` for 5) are accepted, because NEF
  // checksums cover the raw bytes and such files are valid on-chain.
  return { value, consumed };
}

function readVarString(bytes, offset, maxLength) {
  const { value: length, consumed } = readVarInt(bytes, offset);
  if (length > maxLength) {
    throw new NefParseError(`source string exceeds maximum length (${length} > ${maxLength})`, {
      code: "SourceTooLong",
      length,
      max: maxLength,
    });
  }
  const start = offset + consumed;
  const stringBytes = slice(bytes, start, length);
  try {
    return { value: textDecoder.decode(stringBytes), consumed: consumed + length };
  } catch {
    throw new NefParseError(`varstring contains invalid utf-8 at offset ${start}`, {
      code: "InvalidUtf8String",
      offset: start,
    });
  }
}

function readVarBytes(bytes, offset, maxLength) {
  const { value: length, consumed } = readVarInt(bytes, offset);
  if (length > maxLength) {
    throw new NefParseError(`script exceeds maximum size (${length} > ${maxLength})`, {
      code: "ScriptTooLarge",
      length,
      max: maxLength,
    });
  }
  const start = offset + consumed;
  return { value: new Uint8Array(slice(bytes, start, length)), consumed: consumed + length };
}

function slice(bytes, start, length) {
  const end = start + length;
  if (end > bytes.byteLength) {
    throw unexpectedEof(start);
  }
  return bytes.subarray(start, end);
}

function unexpectedEof(offset) {
  return new NefParseError(`unexpected end of data at offset ${offset}`, {
    code: "UnexpectedEof",
    offset,
  });
}
