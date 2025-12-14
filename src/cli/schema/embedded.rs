pub(super) const SCHEMA_VERSION: &str = "1.1.0";

pub(super) const SCHEMA_INFO_PATH: &str = "docs/schema/info.schema.json";
pub(super) const SCHEMA_DISASM_PATH: &str = "docs/schema/disasm.schema.json";
pub(super) const SCHEMA_DECOMPILE_PATH: &str = "docs/schema/decompile.schema.json";
pub(super) const SCHEMA_TOKENS_PATH: &str = "docs/schema/tokens.schema.json";

pub(super) const INFO_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/info.schema.json"
));
pub(super) const DISASM_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/disasm.schema.json"
));
pub(super) const DECOMPILE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/decompile.schema.json"
));
pub(super) const TOKENS_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/schema/tokens.schema.json"
));
