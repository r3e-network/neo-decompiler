use crate::decompiler::helpers::sanitize_identifier;
use crate::instruction::OpCode;

/// A VM operation represented as a call-shaped expression in shared IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intrinsic {
    Opcode(OpCode),
}

impl Intrinsic {
    #[must_use]
    pub fn display_name(self) -> String {
        match self {
            Self::Opcode(opcode) => match opcode {
                OpCode::Setitem => "set_item".to_string(),
                OpCode::Remove => "remove_item".to_string(),
                OpCode::Clearitems => "clear_items".to_string(),
                OpCode::Reverseitems => "reverse_items".to_string(),
                _ => format!("{opcode:?}").to_lowercase(),
            },
        }
    }
}

/// Semantic identity of a call expression, independent of rendered spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SemanticCallTarget {
    Internal {
        offset: usize,
        name: String,
    },
    MethodToken {
        index: usize,
        name: String,
        hash_le: Option<String>,
        call_flags: Option<u8>,
    },
    Syscall {
        hash: u32,
        name: Option<String>,
    },
    Intrinsic(Intrinsic),
    Unresolved {
        display_name: String,
    },
}

impl SemanticCallTarget {
    #[must_use]
    pub fn display_name(&self) -> String {
        match self {
            Self::Internal { name, .. } | Self::Unresolved { display_name: name } => name.clone(),
            Self::MethodToken { name, .. } => sanitize_identifier(name),
            Self::Syscall { .. } => "syscall".to_string(),
            Self::Intrinsic(intrinsic) => intrinsic.display_name(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticCallTarget;

    #[test]
    fn method_token_display_name_is_a_safe_identifier_without_losing_metadata() {
        let target = SemanticCallTarget::MethodToken {
            index: 3,
            name: "9-bad\nname".to_string(),
            hash_le: None,
            call_flags: None,
        };

        assert_eq!(target.display_name(), "_9_bad_name");
        assert!(matches!(
            target,
            SemanticCallTarget::MethodToken { name, .. } if name == "9-bad\nname"
        ));
    }

    #[test]
    fn known_syscall_keeps_name_metadata_and_generic_spelling() {
        let target = SemanticCallTarget::Syscall {
            hash: 0x8CEC_27F8,
            name: Some("System.Runtime.CheckWitness".to_string()),
        };

        assert!(matches!(
            &target,
            SemanticCallTarget::Syscall {
                hash: 0x8CEC_27F8,
                name: Some(name),
            } if name == "System.Runtime.CheckWitness"
        ));
        assert_eq!(target.display_name(), "syscall");
    }
}
