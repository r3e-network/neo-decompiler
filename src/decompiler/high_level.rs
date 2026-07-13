mod emitter;
mod render;

#[cfg(test)]
pub(super) use emitter::HighLevelEmitter;
pub(super) use emitter::MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS;
pub(crate) use render::header::write_contract_header;
pub(crate) use render::render_high_level;
