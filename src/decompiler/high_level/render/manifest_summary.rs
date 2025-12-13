use std::fmt::Write;

use crate::manifest::ContractManifest;

use crate::decompiler::helpers::{
    format_manifest_type, format_permission_entry, manifest_extra_string, sanitize_identifier,
};

pub(super) fn write_manifest_summary(output: &mut String, manifest: &ContractManifest) {
    if !manifest.supported_standards.is_empty() {
        let standards = manifest
            .supported_standards
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(output, "    supported_standards = [{standards}];").unwrap();
    }

    if manifest.features.storage || manifest.features.payable {
        writeln!(output, "    features {{").unwrap();
        if manifest.features.storage {
            writeln!(output, "        storage = true;").unwrap();
        }
        if manifest.features.payable {
            writeln!(output, "        payable = true;").unwrap();
        }
        writeln!(output, "    }}").unwrap();
    }

    if !manifest.permissions.is_empty() {
        writeln!(output, "    permissions {{").unwrap();
        for permission in &manifest.permissions {
            writeln!(output, "        {}", format_permission_entry(permission)).unwrap();
        }
        writeln!(output, "    }}").unwrap();
    }

    if let Some(trusts) = manifest.trusts.as_ref() {
        writeln!(output, "    trusts = {};", trusts.describe()).unwrap();
    }
    if let Some(author) = manifest_extra_string(manifest, "author") {
        writeln!(output, "    // author: {author}").unwrap();
    }
    if let Some(email) = manifest_extra_string(manifest, "email") {
        writeln!(output, "    // email: {email}").unwrap();
    }

    if !manifest.abi.methods.is_empty() {
        writeln!(output, "    // ABI methods").unwrap();
        for method in &manifest.abi.methods {
            let method_name = sanitize_identifier(&method.name);
            let params = method
                .parameters
                .iter()
                .map(|param| {
                    format!(
                        "{}: {}",
                        sanitize_identifier(&param.name),
                        format_manifest_type(&param.kind)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let return_type = format_manifest_type(&method.return_type);
            let mut meta = Vec::new();
            if method_name != method.name {
                meta.push(format!("manifest {:?}", method.name));
            }
            if method.safe {
                meta.push("safe".to_string());
            }
            if let Some(offset) = method.offset {
                meta.push(format!("offset {}", offset));
            }
            let meta_comment = if meta.is_empty() {
                String::new()
            } else {
                format!(" // {}", meta.join(", "))
            };
            writeln!(
                output,
                "    fn {}({}) -> {};{}",
                method_name, params, return_type, meta_comment
            )
            .unwrap();
        }
    }

    if !manifest.abi.events.is_empty() {
        writeln!(output, "    // ABI events").unwrap();
        for event in &manifest.abi.events {
            let params = event
                .parameters
                .iter()
                .map(|param| {
                    format!(
                        "{}: {}",
                        sanitize_identifier(&param.name),
                        format_manifest_type(&param.kind)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let event_name = sanitize_identifier(&event.name);
            let mut meta = Vec::new();
            if event_name != event.name {
                meta.push(format!("manifest {:?}", event.name));
            }
            let meta_comment = if meta.is_empty() {
                String::new()
            } else {
                format!(" // {}", meta.join(", "))
            };
            writeln!(
                output,
                "    event {}({});{}",
                event_name, params, meta_comment
            )
            .unwrap();
        }
    }
}
