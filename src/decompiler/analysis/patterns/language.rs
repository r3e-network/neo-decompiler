/// Infer the source target supported by this decompiler.
///
/// Generated source is intentionally limited to Neo C# for now. Keep the
/// metadata field in `PatternInfo` for report compatibility, but do not claim
/// support for other compiler families until a corresponding renderer exists.
pub(super) fn infer_language(compiler: &str) -> Option<&'static str> {
    let compiler = compiler.trim().to_ascii_lowercase();
    if compiler.is_empty() {
        return None;
    }
    // Neo stores a fixed-width compiler field; fixtures and older toolchains
    // sometimes emit short tags (`cs`, `cs__`) instead of the full product name.
    // Match complete tokens so metadata such as `notcsharp` cannot claim the
    // only source backend this project currently supports.
    if compiler
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| token == "csharp" || token == "cs")
    {
        Some("C#")
    } else {
        None
    }
}

pub(super) fn infer_language_from_source(source: &str) -> Option<&'static str> {
    let source = source.to_ascii_lowercase();
    let source = source.split(['?', '#']).next().unwrap_or_default();
    let filename = source.rsplit(['/', '\\']).next().unwrap_or(source);
    if filename.ends_with(".cs") || filename.ends_with(".csproj") {
        Some("C#")
    } else {
        None
    }
}
