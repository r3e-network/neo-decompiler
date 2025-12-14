use super::super::basic_block::{BasicBlock, BlockId};
use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn create_blocks(&self) -> Vec<BasicBlock> {
        let mut blocks = Vec::new();
        let leaders: Vec<_> = self.leaders.iter().copied().collect();

        for (block_idx, &start_offset) in leaders.iter().enumerate() {
            let end_offset = leaders
                .get(block_idx + 1)
                .copied()
                .unwrap_or_else(|| self.end_offset());

            let start_index = self
                .offset_to_index
                .get(&start_offset)
                .copied()
                .unwrap_or(0);
            let end_index = self
                .offset_to_index
                .get(&end_offset)
                .copied()
                .unwrap_or(self.instructions.len());

            let terminator = self.compute_terminator(start_index, end_index, &leaders);

            blocks.push(BasicBlock::new(
                BlockId::new(block_idx),
                start_offset,
                end_offset,
                start_index..end_index,
                terminator,
            ));
        }

        blocks
    }
}
