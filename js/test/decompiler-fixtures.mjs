import { createHash } from "node:crypto";

export const GAS_TOKEN_HASH = [
  0xcf, 0x76, 0xe2, 0x8b, 0xd0, 0x06, 0x2c, 0x4a, 0x47, 0x8e,
  0xe3, 0x55, 0x61, 0x01, 0x13, 0x19, 0xf3, 0xcf, 0xa4, 0xd2,
];

export const SAMPLE_MANIFEST = JSON.stringify({
  name: "SampleToken",
  groups: [],
  supportedstandards: ["NEP-17"],
  features: { storage: true, payable: false },
  abi: {
    methods: [
      {
        name: "symbol",
        parameters: [],
        returntype: "String",
        offset: 0,
        safe: true,
      },
    ],
    events: [],
  },
  permissions: [],
  trusts: [],
  extra: {},
});

function writeVarint(buffer, value) {
  if (value <= 0xfc) {
    buffer.push(value);
  } else if (value <= 0xffff) {
    buffer.push(0xfd, value & 0xff, value >> 8);
  } else {
    buffer.push(
      0xfe,
      value & 0xff,
      (value >> 8) & 0xff,
      (value >> 16) & 0xff,
      (value >> 24) & 0xff,
    );
  }
}

export function buildSampleNef() {
  const script = [0x10, 0x11, 0x9e, 0x40];
  return buildNefFromScript(script);
}

export function buildNefFromScript(scriptBytes) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  if (script.length === 4 && script[0] === 0x10 && script[1] === 0x11) {
    data.push(1);
    data.push(...GAS_TOKEN_HASH);
    writeVarint(data, 8);
    data.push(...Buffer.from("Transfer"));
    data.push(0x02, 0x00);
    data.push(0x01);
    data.push(0x0f);
  } else {
    data.push(0);
  }
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

export function buildLocalMathNef() {
  const script = [
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, // PUSH1
    0x70, // STLOC0
    0x68, // LDLOC0
    0x12, // PUSH2
    0x9e, // ADD
    0x40, // RET
  ];
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  data.push(0);
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

export function buildNefWithSingleToken(
  scriptBytes,
  hash,
  method,
  parametersCount,
  hasReturnValue,
  callFlags,
) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  data.push(1);
  data.push(...hash);
  writeVarint(data, Buffer.byteLength(method));
  data.push(...Buffer.from(method));
  data.push(parametersCount & 0xff, (parametersCount >> 8) & 0xff);
  data.push(hasReturnValue ? 1 : 0);
  data.push(callFlags);
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

export function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return Array.from(second.subarray(0, 4));
}
