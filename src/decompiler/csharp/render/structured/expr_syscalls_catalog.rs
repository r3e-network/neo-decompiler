//! C# API bindings for syscall hashes recognized by the structured renderer.

#[derive(Clone, Copy)]
pub(super) enum SyscallApi {
    StaticMethod {
        api: &'static str,
        arguments: &'static [SyscallArgument],
    },
    StaticProperty(&'static str),
    InstanceMethod {
        receiver_type: &'static str,
        method: &'static str,
        arguments: &'static [SyscallArgument],
    },
    InstanceProperty {
        receiver_type: &'static str,
        property: &'static str,
    },
    LowLevel,
}

#[derive(Clone, Copy)]
pub(super) enum SyscallArgument {
    Cast(&'static str),
    Int,
    LongInteger,
    Enum(&'static str),
    StorageKey,
    StorageValue,
    Witness,
}

pub(super) fn known_syscall_api(hash: u32) -> Option<SyscallApi> {
    Some(match hash {
        0x0287_99CF => SyscallApi::StaticMethod {
            api: "Contract.CreateStandardAccount",
            arguments: &[SyscallArgument::Cast("ECPoint")],
        },
        0x0388_C3B7 => SyscallApi::StaticProperty("Runtime.Time"),
        0x09E9_336A => SyscallApi::StaticMethod {
            api: "Contract.CreateMultisigAccount",
            arguments: &[SyscallArgument::Int, SyscallArgument::Cast("ECPoint[]")],
        },
        0x0AE3_0C39 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[SyscallArgument::StorageKey, SyscallArgument::StorageValue],
        },
        0x165D_A144 => SyscallApi::LowLevel,
        0x1DBF_54F3 => SyscallApi::InstanceProperty {
            receiver_type: "Iterator",
            property: "Value",
        },
        0x27B3_E756 => SyscallApi::StaticMethod {
            api: "Crypto.CheckSig",
            arguments: &[
                SyscallArgument::Cast("ECPoint"),
                SyscallArgument::Cast("ByteString"),
            ],
        },
        0x28A9_DE6B => SyscallApi::StaticMethod {
            api: "Runtime.GetRandom",
            arguments: &[],
        },
        0x3008_512D => SyscallApi::StaticProperty("Runtime.Transaction"),
        0x31E8_5D92 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0x38E2_B4F9 => SyscallApi::StaticProperty("Runtime.EntryScriptHash"),
        0x3ADC_D09E => SyscallApi::StaticMethod {
            api: "Crypto.CheckMultisig",
            arguments: &[
                SyscallArgument::Cast("ECPoint[]"),
                SyscallArgument::Cast("ByteString[]"),
            ],
        },
        0x3C6E_5339 => SyscallApi::StaticProperty("Runtime.CallingScriptHash"),
        0x4311_2784 => SyscallApi::StaticProperty("Runtime.InvocationCounter"),
        0x525B_7D62 => SyscallApi::StaticMethod {
            api: "Contract.Call",
            arguments: &[
                SyscallArgument::Cast("UInt160"),
                SyscallArgument::Cast("string"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x616F_0195 => SyscallApi::LowLevel,
        0x677B_F71A => SyscallApi::LowLevel,
        0x74A8_FEDB => SyscallApi::StaticProperty("Runtime.ExecutingScriptHash"),
        0x813A_DA95 => SyscallApi::StaticMethod {
            api: "Contract.GetCallFlags",
            arguments: &[],
        },
        0x8418_3FE6 => SyscallApi::StaticMethod {
            api: "Storage.Put",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::StorageValue,
            ],
        },
        0x8B18_F1AC => SyscallApi::StaticMethod {
            api: "Runtime.CurrentSigners",
            arguments: &[],
        },
        0x8CEC_27F8 => SyscallApi::StaticMethod {
            api: "Runtime.CheckWitness",
            arguments: &[SyscallArgument::Witness],
        },
        0x8F80_0CB3 => SyscallApi::StaticMethod {
            api: "Runtime.LoadScript",
            arguments: &[
                SyscallArgument::Cast("ByteString"),
                SyscallArgument::Enum("CallFlags"),
                SyscallArgument::Cast("object[]"),
            ],
        },
        0x93BC_DB2E => SyscallApi::LowLevel,
        0x94F5_5475 => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[SyscallArgument::StorageKey],
        },
        0x9647_E7CF => SyscallApi::StaticMethod {
            api: "Runtime.Log",
            arguments: &[SyscallArgument::Cast("string")],
        },
        0x9AB8_30DF => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0x9CED_089C => SyscallApi::InstanceMethod {
            receiver_type: "Iterator",
            method: "Next",
            arguments: &[],
        },
        0xA038_7DE9 => SyscallApi::StaticProperty("Runtime.Trigger"),
        0xBC8C_5AC3 => SyscallApi::StaticMethod {
            api: "Runtime.BurnGas",
            arguments: &[SyscallArgument::LongInteger],
        },
        0xCE67_F69B => SyscallApi::StaticProperty("Storage.CurrentContext"),
        0xCED8_8814 => SyscallApi::StaticProperty("Runtime.GasLeft"),
        0xDC92_494C => SyscallApi::StaticProperty("Runtime.AddressVersion"),
        0xE0A0_FBC5 => SyscallApi::StaticMethod {
            api: "Runtime.GetNetwork",
            arguments: &[],
        },
        0xE26B_B4F6 => SyscallApi::StaticProperty("Storage.CurrentReadOnlyContext"),
        0xE85E_8DD5 => SyscallApi::StaticMethod {
            api: "Storage.Get",
            arguments: &[SyscallArgument::StorageKey],
        },
        0xE9BF_4C76 => SyscallApi::InstanceProperty {
            receiver_type: "StorageContext",
            property: "AsReadOnly",
        },
        0xEDC5_582F => SyscallApi::StaticMethod {
            api: "Storage.Delete",
            arguments: &[
                SyscallArgument::Cast("StorageContext"),
                SyscallArgument::StorageKey,
            ],
        },
        0xF135_4327 => SyscallApi::StaticMethod {
            api: "Runtime.GetNotifications",
            arguments: &[SyscallArgument::Cast("UInt160")],
        },
        0xF352_7607 => SyscallApi::StaticMethod {
            api: "Storage.Find",
            arguments: &[
                SyscallArgument::StorageKey,
                SyscallArgument::Enum("FindOptions"),
            ],
        },
        0xF6FC_79B2 => SyscallApi::StaticProperty("Runtime.Platform"),
        _ => return None,
    })
}
