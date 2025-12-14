use super::{make_instr, CfgBuilder, OpCode};

#[test]
fn cfg_to_dot_produces_valid_output() {
    let instructions = vec![
        make_instr(0, OpCode::Push0, None),
        make_instr(1, OpCode::Ret, None),
    ];

    let cfg = CfgBuilder::new(&instructions).build();
    let dot = cfg.to_dot();

    assert!(dot.contains("digraph CFG"));
    assert!(dot.contains("BB0"));
}
