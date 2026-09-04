use super::*;

struct TestToken<'a> {
    hash: [u8; 20],
    method: &'a str,
    parameters_count: u16,
    has_return_value: bool,
    call_flags: u8,
}

fn build_nef_with_tokens(script: &[u8], tokens: &[TestToken<'_>]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"NEF3");
    let mut compiler = [0u8; 64];
    compiler[..4].copy_from_slice(b"test");
    data.extend_from_slice(&compiler);
    data.push(0); // source
    data.push(0); // reserved byte
    write_varint(&mut data, tokens.len() as u32);
    for token in tokens {
        data.extend_from_slice(&token.hash);
        write_varint(&mut data, token.method.len() as u32);
        data.extend_from_slice(token.method.as_bytes());
        data.extend_from_slice(&token.parameters_count.to_le_bytes());
        data.push(u8::from(token.has_return_value));
        data.push(token.call_flags);
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    write_varint(&mut data, script.len() as u32);
    data.extend_from_slice(script);
    let checksum = NefParser::calculate_checksum(&data);
    data.extend_from_slice(&checksum.to_le_bytes());
    data
}

#[test]
fn callt_arguments_keep_producer_side_effects_in_vm_order() {
    let tokens = [
        TestToken {
            hash: [0x11; 20],
            method: "first",
            parameters_count: 0,
            has_return_value: true,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x22; 20],
            method: "second",
            parameters_count: 0,
            has_return_value: true,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x33; 20],
            method: "consume",
            parameters_count: 2,
            has_return_value: false,
            call_flags: 0x0F,
        },
    ];
    let script = [0x37, 0, 0, 0x37, 1, 0, 0x37, 2, 0, 0x40];
    let nef = build_nef_with_tokens(&script, &tokens);

    for trace in [true, false] {
        let output = Decompiler::new()
            .with_trace_comments(trace)
            .decompile_bytes_with_manifest(&nef, None, OutputFormat::HighLevel)
            .expect("ordered CALLT rendering succeeds");
        let source = output.high_level.expect("high-level output");
        let first = source.find("let t0 = __callt_token_0();").expect(&source);
        let second = source.find("let t1 = __callt_token_1();").expect(&source);
        let consume = source.find("__callt_token_2(t1, t0);").expect(&source);
        assert!(first < second && second < consume, "{source}");
        assert_eq!(source.matches("__callt_token_0()").count(), 1, "{source}");
        assert_eq!(source.matches("__callt_token_1()").count(), 1, "{source}");
    }
}

#[test]
fn callt_does_not_delay_an_ambient_value_producer_until_after_a_void_call() {
    let tokens = [
        TestToken {
            hash: [0x11; 20],
            method: "read",
            parameters_count: 0,
            has_return_value: true,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x22; 20],
            method: "write",
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        },
    ];
    let nef = build_nef_with_tokens(&[0x37, 0, 0, 0x37, 1, 0, 0x40], &tokens);
    let output = Decompiler::new()
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(&nef, None, OutputFormat::HighLevel)
        .expect("ambient CALLT rendering succeeds");
    let source = output.high_level.expect("high-level output");
    let read = source.find("let t0 = __callt_token_0();").expect(&source);
    let write = source.find("__callt_token_1();").expect(&source);
    let returned = source.find("return t0;").expect(&source);
    assert!(read < write && write < returned, "{source}");
    assert_eq!(source.matches("__callt_token_0()").count(), 1, "{source}");
}

#[test]
fn callt_display_limit_preserves_all_omitted_argument_producer_effects() {
    const PRODUCERS: usize = 300;
    let tokens = [
        TestToken {
            hash: [0x11; 20],
            method: "produce",
            parameters_count: 0,
            has_return_value: true,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x22; 20],
            method: "consume",
            parameters_count: PRODUCERS as u16,
            has_return_value: false,
            call_flags: 0x0F,
        },
    ];
    let mut script = [0x37, 0, 0].repeat(PRODUCERS);
    script.extend_from_slice(&[0x37, 1, 0, 0x40]);
    let output = Decompiler::new()
        .with_trace_comments(false)
        .decompile_bytes_with_manifest(
            &build_nef_with_tokens(&script, &tokens),
            None,
            OutputFormat::HighLevel,
        )
        .expect("bounded CALLT rendering succeeds");
    let source = output.high_level.expect("high-level output");
    assert_eq!(source.matches("__callt_token_0()").count(), PRODUCERS);
    let consume = source.find("__callt_token_1(t299, ").expect(&source);
    assert!(source[..consume].contains("let t299 = __callt_token_0();"));
    assert_eq!(
        source
            .matches("unknown /* 44 missing/omitted arguments */")
            .count(),
        1
    );
    assert!(!source.contains("???"));
}

#[test]
fn untrusted_callt_names_use_unique_index_labels_and_safe_csharp_strings() {
    let hostile = "x); Evil(); // \"quoted\"\n\u{202E}";
    let tokens = [
        TestToken {
            hash: [0x42; 20],
            method: hostile,
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x11; 20],
            method: "transfer",
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        },
        TestToken {
            hash: [0x22; 20],
            method: "transfer",
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        },
        // A duplicate row must still retain its own index identity.
        TestToken {
            hash: [0x11; 20],
            method: "transfer",
            parameters_count: 0,
            has_return_value: false,
            call_flags: 0x0F,
        },
    ];
    let script = [
        0x37, 0x00, 0x00, // CALLT 0
        0x37, 0x01, 0x00, // CALLT 1
        0x37, 0x02, 0x00, // CALLT 2
        0x37, 0x03, 0x00, // CALLT 3
        0x40, // RET
    ];
    let decompilation = Decompiler::new()
        .decompile_bytes(&build_nef_with_tokens(&script, &tokens))
        .expect("decompile succeeds");
    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    let csharp = decompilation.csharp.as_deref().expect("C# output");

    for index in 0..tokens.len() {
        assert!(
            high_level.contains(&format!("__callt_token_{index}();")),
            "token {index} lost its index label:\n{high_level}"
        );
    }
    assert!(!high_level.contains("x); Evil(); // \"quoted\"("));
    assert!(!high_level.contains("transfer();"));

    assert!(
        csharp.contains("\"x); Evil(); // \\\"quoted\\\"\\n\\u202E\""),
        "{csharp}"
    );
    assert!(!csharp
        .lines()
        .any(|line| line.trim_start().starts_with("Evil();")));
    assert_eq!(
        csharp
            .lines()
            .filter(|line| line.contains("0x11, 0x11") && line.contains("\"transfer\""))
            .count(),
        2,
        "{csharp}"
    );
    assert_eq!(
        csharp
            .lines()
            .filter(|line| line.contains("0x22, 0x22") && line.contains("\"transfer\""))
            .count(),
        1,
        "{csharp}"
    );
}

#[test]
fn exact_unrestricted_native_callt_keeps_qualified_label() {
    let stdlib_hash = [
        0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A, 0x79, 0xE1, 0x44,
        0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
    ];
    let nef = build_nef_with_single_token(
        &[0x11, 0x37, 0x00, 0x00, 0x40],
        stdlib_hash,
        "Serialize",
        1,
        true,
        0x0F,
    );
    let decompilation = Decompiler::new()
        .decompile_bytes(&nef)
        .expect("decompile succeeds");
    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    let csharp = decompilation.csharp.as_deref().expect("C# output");

    assert!(high_level.contains("StdLib::Serialize(t0)"), "{high_level}");
    assert!(csharp.contains("StdLib.Serialize(1)"), "{csharp}");
    assert!(!high_level.contains("__callt_token_0("));
}

#[test]
fn manifest_method_cannot_collide_with_reserved_callt_label() {
    let nef = build_nef_with_single_token(
        &[0x37, 0x00, 0x00, 0x40],
        [0x33; 20],
        "external",
        0,
        true,
        0x0F,
    );
    let manifest = ContractManifest::from_json_str(
        r#"{
            "name": "ReservedLabels",
            "abi": {
                "methods": [{
                    "name": "__callt_token_0",
                    "parameters": [],
                    "returntype": "Any",
                    "offset": 0
                }],
                "events": []
            },
            "permissions": [],
            "trusts": "*"
        }"#,
    )
    .expect("manifest parsed");
    let decompilation = Decompiler::new()
        .decompile_bytes_with_manifest(&nef, Some(manifest), OutputFormat::All)
        .expect("decompile succeeds");
    let high_level = decompilation
        .high_level
        .as_deref()
        .expect("high-level output");
    let csharp = decompilation.csharp.as_deref().expect("C# output");

    assert!(
        high_level.contains("fn method___callt_token_0() -> any {"),
        "{high_level}"
    );
    assert!(
        high_level.contains("return __callt_token_0();"),
        "{high_level}"
    );
    assert!(csharp.contains("method___callt_token_0()"), "{csharp}");
    assert!(csharp.contains("Contract.Call("), "{csharp}");
    assert!(
        !csharp.contains("static object __callt_token_0()"),
        "{csharp}"
    );
}
