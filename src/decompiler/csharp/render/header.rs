use std::fmt::Write;

use crate::manifest::ContractManifest;
use crate::nef::NefFile;
use crate::util;

use super::super::super::helpers::{format_permission_entry, manifest_extra_string};
use super::super::helpers::escape_csharp_string;

pub(super) fn write_preamble(output: &mut String) {
    writeln!(output, "using System;").unwrap();
    writeln!(output, "using System.Numerics;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Attributes;").unwrap();
    writeln!(output, "using Neo.SmartContract.Framework.Services;").unwrap();
    writeln!(output).unwrap();
}

pub(super) fn write_contract_open(
    output: &mut String,
    contract_name: &str,
    nef: &NefFile,
    manifest: Option<&ContractManifest>,
) {
    writeln!(output, "namespace NeoDecompiler.Generated {{").unwrap();
    if let Some(manifest) = manifest {
        if let Some(author) = manifest_extra_string(manifest, "author") {
            writeln!(
                output,
                "    [ManifestExtra(\"Author\", \"{}\")]",
                escape_csharp_string(&author)
            )
            .unwrap();
        }
        if let Some(email) = manifest_extra_string(manifest, "email") {
            writeln!(
                output,
                "    [ManifestExtra(\"Email\", \"{}\")]",
                escape_csharp_string(&email)
            )
            .unwrap();
        }
    }
    writeln!(output, "    public class {contract_name} : SmartContract").unwrap();
    writeln!(output, "    {{").unwrap();

    let script_hash = nef.script_hash();
    writeln!(
        output,
        "        // script hash (little-endian): {}",
        util::format_hash(&script_hash)
    )
    .unwrap();
    writeln!(
        output,
        "        // script hash (big-endian): {}",
        util::format_hash_be(&script_hash)
    )
    .unwrap();

    if let Some(manifest) = manifest {
        if !manifest.supported_standards.is_empty() {
            let standards = manifest.supported_standards.join(", ");
            writeln!(output, "        // supported standards: {standards}").unwrap();
        }
        if manifest.features.storage || manifest.features.payable {
            writeln!(output, "        // features:").unwrap();
            if manifest.features.storage {
                writeln!(output, "        //   storage = true").unwrap();
            }
            if manifest.features.payable {
                writeln!(output, "        //   payable = true").unwrap();
            }
        }
        if !manifest.permissions.is_empty() {
            writeln!(output, "        // permissions:").unwrap();
            for permission in &manifest.permissions {
                writeln!(
                    output,
                    "        //   {}",
                    format_permission_entry(permission)
                )
                .unwrap();
            }
        }
        if let Some(trusts) = manifest.trusts.as_ref() {
            writeln!(output, "        // trusts = {}", trusts.describe()).unwrap();
        }
    } else {
        writeln!(output, "        // manifest not provided").unwrap();
    }

    writeln!(output).unwrap();
}

pub(super) fn write_contract_close(output: &mut String) {
    writeln!(output, "    }}").unwrap();
    writeln!(output, "}}").unwrap();
}
