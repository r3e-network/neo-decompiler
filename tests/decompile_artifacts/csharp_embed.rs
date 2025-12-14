pub(crate) const MANIFEST_PREFIX: &str = "ContractManifest.Parse(@\"";
pub(crate) const MANIFEST_SUFFIX: &str = "\");";
pub(crate) const NEF_PREFIX: &str = "Convert.FromBase64String(@\"";
pub(crate) const NEF_SUFFIX: &str = "\")";

pub(crate) fn extract_section<'a>(source: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let start = source.find(prefix)? + prefix.len();
    let rest = &source[start..];
    let end = rest.find(suffix)?;
    Some(rest[..end].trim())
}

pub(crate) fn unescape_verbatim(input: &str) -> String {
    input.replace("\"\"", "\"")
}
