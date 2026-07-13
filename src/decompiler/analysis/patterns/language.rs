pub(super) fn infer_language(compiler: &str) -> Option<&'static str> {
    let compiler = compiler.to_ascii_lowercase();
    if compiler.contains("csharp") || compiler.contains("neo.compiler") {
        Some("C#")
    } else if compiler.contains("boa") || compiler.contains("python") {
        Some("Python")
    } else if compiler.contains("neogo") || compiler.contains("neo-go") {
        Some("Go")
    } else if compiler.contains("rust") {
        Some("Rust")
    } else if compiler.contains("typescript") || compiler.contains("javascript") {
        Some("TypeScript/JavaScript")
    } else if compiler.contains("java") {
        Some("Java")
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
    } else if filename.ends_with(".py") {
        Some("Python")
    } else if filename.ends_with(".go") {
        Some("Go")
    } else if filename.ends_with(".rs") {
        Some("Rust")
    } else if filename.ends_with(".java") {
        Some("Java")
    } else if filename.ends_with(".ts")
        || filename.ends_with(".tsx")
        || filename.ends_with(".js")
        || filename.ends_with(".jsx")
    {
        Some("TypeScript/JavaScript")
    } else {
        None
    }
}
