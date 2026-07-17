const TYPE_MAP = new Map([
  ["void", "void"],
  ["bool", "bool"],
  ["boolean", "bool"],
  ["int", "BigInteger"],
  ["integer", "BigInteger"],
  ["string", "string"],
  ["hash160", "UInt160"],
  ["hash256", "UInt256"],
  ["publickey", "ECPoint"],
  ["bytes", "ByteString"],
  ["bytestring", "ByteString"],
  ["bytearray", "ByteString"],
  ["signature", "ByteString"],
  ["array", "object[]"],
  ["map", "Map<object, object>"],
  ["interop", "object"],
  ["interopinterface", "object"],
  ["any", "object"],
]);

export function csharpType(type) {
  return TYPE_MAP.get(String(type ?? "any").trim().toLowerCase()) ?? "object";
}
