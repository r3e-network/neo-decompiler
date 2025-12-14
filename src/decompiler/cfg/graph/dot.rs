use super::{Cfg, EdgeKind};

impl Cfg {
    /// Export CFG to DOT format for visualization.
    pub fn to_dot(&self) -> String {
        let mut dot = String::from("digraph CFG {\n");
        dot.push_str("  node [shape=box];\n");

        let reachable = self.reachable_blocks();
        for block in self.blocks.values() {
            let label = format!(
                "{}\\n[{:#06X}-{:#06X})\\n{} instrs",
                block.id,
                block.start_offset,
                block.end_offset,
                block.instruction_count()
            );
            let style = if !reachable.contains(&block.id) {
                ", style=filled, fillcolor=lightgray"
            } else if block.id == self.entry {
                ", style=filled, fillcolor=lightgreen"
            } else if self.exits.contains(&block.id) {
                ", style=filled, fillcolor=lightcoral"
            } else {
                ""
            };
            dot.push_str(&format!("  {} [label=\"{}\"{}];\n", block.id, label, style));
        }

        for edge in &self.edges {
            let style = match edge.kind {
                EdgeKind::Unconditional => "",
                EdgeKind::ConditionalTrue => " [color=green, label=\"T\"]",
                EdgeKind::ConditionalFalse => " [color=red, label=\"F\"]",
                EdgeKind::Exception => " [color=orange, style=dashed, label=\"exc\"]",
                EdgeKind::Finally => " [color=blue, style=dashed, label=\"finally\"]",
            };
            dot.push_str(&format!("  {} -> {}{};\n", edge.from, edge.to, style));
        }

        dot.push_str("}\n");
        dot
    }
}
