using System;
using System.Numerics;
using Neo.SmartContract.Framework;
using Neo.SmartContract.Framework.Attributes;
using Neo.SmartContract.Framework.Services;

namespace NeoDecompiler.Generated {
    public class NeoContract : SmartContract
    {
        // script hash (little-endian): 7E5D562519A98333B104ED1067ABA18C4918F137
        // script hash (big-endian): 37F118498CA1AB6710ED04B13383A91925565D7E
        // manifest not provided

        public static void ScriptEntry()
        {
            // 0000: INITSLOT
            // declare 1 locals, 0 arguments
            // 0003: PUSH0
            var t0 = 0;
            // 0004: STLOC0

            // 0005: LDLOC0
            // 0006: PUSH3
            var t1 = 3;
            // 0007: LT

            // 0008: JMPIFNOT
            for (var loc0 = t0; loc0 < t1; loc0 = loc0 + 1) {
            // 000A: noop
            // 000B: LDLOC0
            // 000C: PUSH1

            // 000D: ADD

            // 000E: STLOC0

            }
            // 0011: RET
            return;
        }
    }
}
