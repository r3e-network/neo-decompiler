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
const MAX_SOURCE_LEN = 256;
const MAX_METHOD_TOKENS = 256;
const MAX_SCRIPT_LEN = 0x8_0000;
const MAX_METHOD_NAME_LEN = 1024;
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
  const magicBytes = slice(bytes, offset, 4);
  const actualMagic = textDecoder.decode(magicBytes);
  if (actualMagic !== MAGIC) {
    throw new NefParseError(
      `invalid magic bytes: expected ${JSON.stringify(MAGIC)}, got ${JSON.stringify(actualMagic)}`,
      { code: "InvalidMagic", expected: MAGIC, actual: actualMagic },
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
      magic: actualMagic,
      compiler,
      source,
    },
    methodTokens,
    script,
    checksum,
    scriptHash: upperHex(scriptHashBytes),
    scriptHashLE: upperHexReversed(scriptHashBytes),
  };
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

  if (consumed !== varIntEncodedLength(value)) {
    throw new NefParseError(`varint is not canonically encoded at offset ${offset}`, {
      code: "NonCanonicalVarInt",
      offset,
    });
  }

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

function varIntEncodedLength(value) {
  if (value <= 0xfc) return 1;
  if (value <= 0xffff) return 3;
  if (value <= 0xffffffff) return 5;
  return 9;
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
