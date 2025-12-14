use super::super::basic_block::{BasicBlock, Terminator};
use super::super::graph::{Cfg, EdgeKind};
use super::CfgBuilder;

impl<'a> CfgBuilder<'a> {
    pub(super) fn build_cfg(&self, blocks: Vec<BasicBlock>) -> Cfg {
        let mut cfg = Cfg::new();

        for block in &blocks {
            cfg.add_block(block.clone());
        }

        for block in &blocks {
            match &block.terminator {
                Terminator::Fallthrough { target } => {
                    cfg.add_edge(block.id, *target, EdgeKind::Unconditional);
                }
                Terminator::Jump { target } => {
                    cfg.add_edge(block.id, *target, EdgeKind::Unconditional);
                }
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    cfg.add_edge(block.id, *then_target, EdgeKind::ConditionalTrue);
                    cfg.add_edge(block.id, *else_target, EdgeKind::ConditionalFalse);
                }
                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                } => {
                    cfg.add_edge(block.id, *body_target, EdgeKind::Unconditional);
                    if let Some(c) = catch_target {
                        cfg.add_edge(block.id, *c, EdgeKind::Exception);
                    }
                    if let Some(f) = finally_target {
                        cfg.add_edge(block.id, *f, EdgeKind::Finally);
                    }
                }
                Terminator::EndTry { continuation } => {
                    cfg.add_edge(block.id, *continuation, EdgeKind::Unconditional);
                }
                Terminator::Return
                | Terminator::Throw
                | Terminator::Abort
                | Terminator::Unknown => {}
            }
        }

        cfg
    }
}
