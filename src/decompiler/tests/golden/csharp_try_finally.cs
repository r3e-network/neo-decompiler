using System;
using System.Numerics;
using Neo.SmartContract.Framework;
using Neo.SmartContract.Framework.Attributes;
using Neo.SmartContract.Framework.Services;

namespace NeoDecompiler.Generated {
    public class NeoContract : SmartContract
    {
        // script hash (little-endian): B9B02D58D4E203558865B169D0BFBA7E3DA868D8
        // script hash (big-endian): D868A83D7EBABFD069B165885503E2D4582DB0B9
        // manifest not provided

        public static void ScriptEntry()
        {
            // 0000: TRY
            try {
            // 0003: PUSH1
            var t0 = 1;
            }
            finally {
            // 0006: PUSH2
            var t1 = 2;
            }
            // 0008: RET
            return t1;
        }
    }
}
