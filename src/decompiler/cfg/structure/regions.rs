use super::*;
impl<'a> StructCtx<'a> {
    pub(super) fn structure_irreducible(
        &self,
        entry: BlockId,
        region: &BTreeSet<BlockId>,
    ) -> IrBlock {
        let mut out = IrBlock::new();

        if region.contains(&entry) {
            out.push(Stmt::Goto(crate::decompiler::ir::BlockLabel(entry.0)));
        } else {
            self.emit_irreducible_entry(&mut out, entry, region);
        }

        for block in region {
            out.push(Stmt::Label(crate::decompiler::ir::BlockLabel(block.0)));
            let terminator = self.terminator(*block);
            if matches!(terminator, Terminator::Branch { .. }) {
                self.emit_body_except_condition(&mut out, *block);
            } else {
                self.emit_body(&mut out, *block);
            }
            match terminator {
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    let condition = self
                        .comparison_condition_for_block(*block)
                        .unwrap_or_else(|| self.condition_for_block(*block));
                    out.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                        condition,
                        self.irreducible_edge_block(then_target, region),
                        self.irreducible_edge_block(else_target, region),
                    ))));
                }
                Terminator::Fallthrough { target } | Terminator::Jump { target } => {
                    out.stmts
                        .extend(self.irreducible_edge_block(target, region).stmts);
                }
                Terminator::Return
                | Terminator::Throw
                | Terminator::Abort
                | Terminator::NoReturnCall
                | Terminator::EndFinally { .. }
                | Terminator::Unknown => {}
                Terminator::TryEntry { .. }
                | Terminator::EndTry { .. }
                | Terminator::EndTryFinally { .. } => {}
            }
        }
        out
    }

    fn emit_irreducible_entry(
        &self,
        out: &mut IrBlock,
        entry: BlockId,
        region: &BTreeSet<BlockId>,
    ) {
        let Some(block) = self.cfg.block(entry) else {
            return;
        };
        if matches!(&block.terminator, Terminator::Branch { .. }) {
            self.emit_body_except_condition(out, entry);
        } else {
            self.emit_body(out, entry);
        }
        match block.terminator.clone() {
            Terminator::Branch {
                then_target,
                else_target,
            } => {
                let condition = self
                    .comparison_condition_for_block(entry)
                    .unwrap_or_else(|| self.condition_for_block(entry));
                out.push(Stmt::ControlFlow(Box::new(ControlFlow::if_else(
                    condition,
                    self.irreducible_edge_block(then_target, region),
                    self.irreducible_edge_block(else_target, region),
                ))));
            }
            Terminator::Fallthrough { target } | Terminator::Jump { target } => {
                out.stmts
                    .extend(self.irreducible_edge_block(target, region).stmts);
            }
            _ => {}
        }
    }

    fn irreducible_edge_block(&self, target: BlockId, region: &BTreeSet<BlockId>) -> IrBlock {
        if region.contains(&target) {
            return IrBlock::with_stmts(vec![Stmt::Goto(crate::decompiler::ir::BlockLabel(
                target.0,
            ))]);
        }
        let mut blocked = region.iter().copied().collect::<HashSet<_>>();
        self.structure_region(target, None, &mut blocked, true)
    }

    /// Structure the region reachable from `entry`, stopping without traversing
    /// into `boundary` (used so an `if`'s then/else sub-regions halt at the
    /// merge block, which the outer loop then emits in sequence).
    pub(super) fn structure_region(
        &self,
        entry: BlockId,
        boundary: Option<BlockId>,
        visited: &mut HashSet<BlockId>,
        detect_dowhile: bool,
    ) -> IrBlock {
        let mut out = IrBlock::new();
        let mut cur = Some(entry);

        while let Some(bid) = cur {
            if Some(bid) == boundary {
                break;
            }

            // do-while: a loop header whose own terminator is NOT a Branch (the
            // test sits at the bottom, on the latch). Detect it at the header so
            // the whole body is collected before the test, then resume at the
            // latch's exit. `detect_dowhile` is cleared for the body sub-walk so
            // the header isn't re-detected on re-entry.
            if detect_dowhile
                && self.loop_headers.contains(&bid)
                && !matches!(self.terminator(bid), Terminator::Branch { .. })
            {
                if let Some((latch, exit)) = self.find_dowhile_latch(bid) {
                    let mut body = self.structure_region(bid, Some(latch), visited, false);
                    self.emit_body_except_condition(&mut body, latch);
                    let backedge_copies = self.phi_lowering.edge_statements(latch, bid);
                    if !backedge_copies.is_empty() {
                        let first_iteration = self
                            .phi_lowering
                            .fresh_name(&format!("do_while_first_{}", bid.index()));
                        out.push(Stmt::assign(
                            first_iteration.clone(),
                            Expr::Literal(crate::decompiler::ir::Literal::Bool(true)),
                        ));
                        body.stmts.splice(
                            0..0,
                            [
                                Stmt::ControlFlow(Box::new(ControlFlow::if_then(
                                    Expr::unary(
                                        crate::decompiler::ir::UnaryOp::LogicalNot,
                                        Expr::var(first_iteration.clone()),
                                    ),
                                    IrBlock::with_stmts(backedge_copies),
                                ))),
                                Stmt::assign(
                                    first_iteration,
                                    Expr::Literal(crate::decompiler::ir::Literal::Bool(false)),
                                ),
                            ],
                        );
                    }
                    let cond = self.condition_for_block(latch);
                    out.push(Stmt::ControlFlow(Box::new(ControlFlow::do_while(
                        body, cond,
                    ))));
                    out.stmts
                        .extend(self.phi_lowering.edge_statements(latch, exit));
                    // The latch's test is consumed by the do-while; mark it
                    // visited so the outer walk does not re-emit it, and resume
                    // at its exit.
                    visited.insert(latch);
                    cur = Some(exit);
                    continue;
                }
                if let Some(latch) = self.find_unconditional_latch(bid) {
                    let mut body = self.structure_region(bid, Some(latch), visited, false);
                    self.emit_body(&mut body, latch);
                    body.stmts
                        .extend(self.phi_lowering.edge_statements(latch, bid));
                    out.push(Stmt::ControlFlow(Box::new(ControlFlow::while_loop(
                        Expr::Literal(crate::decompiler::ir::Literal::Bool(true)),
                        body,
                    ))));
                    visited.insert(latch);
                    cur = None;
                    continue;
                }
            }

            if !visited.insert(bid) {
                break;
            }
            if self.leave_targets.contains(&bid) {
                out.push(Stmt::Label(crate::decompiler::ir::BlockLabel(bid.0)));
            }

            if let Terminator::Branch {
                then_target,
                else_target,
            } = self.terminator(bid)
            {
                if self.try_emit_infinite_branch_loop(
                    &mut out,
                    bid,
                    then_target,
                    else_target,
                    visited,
                ) {
                    cur = None;
                    continue;
                }
            }

            // For a Branch, the last def IS the branch condition and is
            // consumed (inlined) into the `if (…)` head by handle_branch /
            // the loop-header path — emit all-but-last to avoid duplicating it.
            // For other terminators (Return / Jump / Fallthrough / TryEntry /
            // EndTry / Unknown) the last def is regular body code and is
            // emitted in full.
            if matches!(self.terminator(bid), Terminator::Branch { .. }) {
                self.emit_body_except_condition(&mut out, bid);
            } else {
                self.emit_body(&mut out, bid);
            }

            cur = match self.terminator(bid) {
                Terminator::Return => {
                    if !self.block_has_explicit_return(bid) {
                        out.push(Stmt::Comment(format!("return at {:?}", bid)));
                    }
                    None
                }
                Terminator::Throw | Terminator::Abort => {
                    if !self.block_has_explicit_failure(bid) {
                        out.push(Stmt::Comment(format!("return/throw/abort at {:?}", bid)));
                    }
                    None
                }
                Terminator::NoReturnCall => None,
                Terminator::Unknown => {
                    out.push(Stmt::Comment(format!("return/throw/abort at {:?}", bid)));
                    None
                }
                Terminator::Fallthrough { target } | Terminator::Jump { target } => Some(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                } => {
                    // A loop header branch (has a back-edge predecessor) is a
                    // `while`. Either successor may be the body; orient it from
                    // the back-edge and negate the condition when the true edge
                    // is the loop exit.
                    if self.loop_headers.contains(&bid) {
                        let cond = self
                            .comparison_condition_for_block(bid)
                            .unwrap_or_else(|| self.condition_for_block(bid));
                        let (body_target, follow_target, cond) =
                            self.orient_branch_loop(bid, then_target, else_target, cond);
                        let mut body = self.structure_edge_region(
                            bid,
                            body_target,
                            Some(follow_target),
                            visited,
                        );
                        self.build_loop(&mut out, bid, cond, &mut body);
                        out.stmts
                            .extend(self.phi_lowering.edge_statements(bid, follow_target));
                        Some(follow_target)
                    } else {
                        self.handle_branch(
                            &mut out,
                            bid,
                            then_target,
                            else_target,
                            boundary,
                            visited,
                        )
                    }
                }
                Terminator::TryEntry {
                    body_target,
                    catch_target,
                    finally_target,
                } => {
                    let has_nonlocal_leave =
                        self.try_has_nonlocal_leave(bid, body_target, catch_target);
                    let continuation = self.handle_try(
                        &mut out,
                        bid,
                        body_target,
                        catch_target,
                        finally_target,
                        visited,
                    );
                    if has_nonlocal_leave {
                        None
                    } else {
                        continuation
                    }
                }
                Terminator::EndTry {
                    continuation,
                    nonlocal,
                } => {
                    if nonlocal {
                        out.push(self.leave_transfer(bid, continuation));
                        None
                    } else {
                        Some(continuation)
                    }
                }
                Terminator::EndTryFinally {
                    continuation,
                    nonlocal,
                    ..
                } => {
                    if nonlocal {
                        out.push(self.leave_transfer(bid, continuation));
                        None
                    } else {
                        Some(continuation)
                    }
                }
                // The owning `handle_try` resumes at the recovered ENDTRY
                // continuation after the whole finally arm is structured.
                Terminator::EndFinally { .. } => None,
            };
        }

        out
    }
}
