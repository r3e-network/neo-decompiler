//! Generated C# helper declarations used by lifted VM bodies.

use std::fmt::Write;

use super::super::super::super::helpers::stack_item_type_tag;
use super::super::structured::plan::csharp_type;
use super::super::TaggedOpcodeHelper;

pub(crate) fn write_vm_exception_type(output: &mut String, type_name: Option<&str>) {
    let Some(type_name) = type_name else {
        return;
    };
    writeln!(
        output,
        "        private sealed class {type_name} : Exception"
    )
    .unwrap();
    writeln!(output, "        {{").unwrap();
    writeln!(output, "            internal dynamic Payload {{ get; }}").unwrap();
    writeln!(
        output,
        "            internal {type_name}(dynamic payload) : base(Convert.ToString((object)payload))"
    )
    .unwrap();
    writeln!(output, "            {{").unwrap();
    writeln!(output, "                Payload = payload;").unwrap();
    writeln!(output, "            }}").unwrap();
    writeln!(output, "        }}").unwrap();
    writeln!(output).unwrap();
}

pub(crate) fn write_assert_message_helper(output: &mut String, helper_name: Option<&str>) {
    let Some(helper_name) = helper_name else {
        return;
    };
    writeln!(
        output,
        "        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.ASSERTMSG)]"
    )
    .unwrap();
    writeln!(
        output,
        "        private static extern void {helper_name}(bool condition, string message);"
    )
    .unwrap();
    writeln!(output).unwrap();
}

pub(crate) fn write_bare_throw_helper(output: &mut String, helper_name: Option<&str>) {
    let Some(helper_name) = helper_name else {
        return;
    };
    writeln!(
        output,
        "        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.THROW)]"
    )
    .unwrap();
    writeln!(
        output,
        "        private static extern void {helper_name}();"
    )
    .unwrap();
    writeln!(output).unwrap();
}

pub(crate) fn write_unpack_packstruct_helper(output: &mut String, helper_name: Option<&str>) {
    let Some(helper_name) = helper_name else {
        return;
    };
    writeln!(
        output,
        "        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.UNPACK)]"
    )
    .unwrap();
    writeln!(
        output,
        "        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.PACKSTRUCT)]"
    )
    .unwrap();
    writeln!(
        output,
        "        private static extern object[] {helper_name}(object value);"
    )
    .unwrap();
    writeln!(output).unwrap();
}

pub(crate) fn write_tagged_opcode_helpers(output: &mut String, helpers: &[TaggedOpcodeHelper]) {
    for helper in helpers {
        let tag = stack_item_type_tag(helper.target)
            .expect("planned tagged opcode helper has a VM type tag");
        let return_type = if helper.opcode == crate::instruction::OpCode::Istype {
            "bool"
        } else {
            csharp_type(helper.target, true)
        };
        writeln!(
            output,
            "        [global::Neo.SmartContract.Framework.Attributes.OpCode(global::Neo.SmartContract.Framework.OpCode.{}, \"{tag:02X}\")]",
            helper.opcode.mnemonic()
        )
        .unwrap();
        writeln!(
            output,
            "        private static extern {return_type} {}(object value);",
            helper.name
        )
        .unwrap();
    }
    if !helpers.is_empty() {
        writeln!(output).unwrap();
    }
}

pub(crate) fn write_unresolved_call_helper(output: &mut String) {
    writeln!(
        output,
        "        private static dynamic __NeoDecompilerUnresolvedCall(string name, object[] args) => throw new NotImplementedException($\"Unresolved Neo VM call: {{name}}\");"
    )
    .unwrap();
    writeln!(output).unwrap();
}
