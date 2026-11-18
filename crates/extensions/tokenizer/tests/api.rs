//! Verify numeric constants, exact token names and native argument handling against PHP.

mod support;

use phera_extension_api::{
    ArrayKey, Value,
    testing::{TestContext, call_method, instantiate, oracle},
};

use support::Matrix;

#[test]
fn constants_and_all_names_match_php() {
    let mut matrix = Matrix::new("tokenizer-names");
    let observed = oracle::run_php(
        "tokenizer-constants",
        r#"<?php
        foreach (get_defined_constants(true)['tokenizer'] as $name => $id) {
            echo "$name=$id\n";
        }
        "#,
    )
    .expect("PHP 8.5 required");
    let mut actual: Vec<_> = matrix
        .registry
        .constants()
        .filter(|(name, _)| name.starts_with("T_") || *name == "TOKEN_PARSE")
        .map(|(name, id)| {
            format!(
                "{name}={}",
                match id.to_value() {
                    Value::Int(id) => id,
                    _ => panic!("tokenizer constant must be an int"),
                }
            )
        })
        .collect();
    let mut expected = observed;
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);

    for id in (0..=1000).chain([i64::MIN, -1, 1 << 32 | 262, i64::MAX]) {
        matrix.check(
            format!("token_name({id})"),
            "token_name",
            vec![Value::Int(id)],
        );
    }
    let php = oracle::run_php("tokenizer-names", &matrix.php).expect("PHP 8.5 required");
    oracle::compare("token_name", &matrix.labels, &php, &matrix.results);
    println!(
        "{} constants and {} name calls matched",
        actual.len(),
        matrix.labels.len()
    );
}

#[test]
fn functions_and_methods_match_php() {
    let mut matrix = Matrix::new("tokenizer-api");
    let arguments = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(i64::MIN),
        Value::Int(262),
        Value::Int(i64::MAX),
        Value::Float(-0.0),
        Value::Float(262.5),
        Value::from("262"),
        Value::from("no"),
        Value::from("<?php echo 42;"),
        Value::from(&b"<?php echo '\xff';"[..]),
        Value::from(phera_extension_api::PhpArray::new()),
    ];
    for name in ["token_get_all", "PhpToken::tokenize", "token_name"] {
        matrix.check(format!("{name}()"), name, vec![]);
        for value in &arguments {
            matrix.check(format!("{name}({value:?})"), name, vec![value.clone()]);
            for flags in &arguments {
                let label = format!("{name}({value:?}, {flags:?})");
                matrix.check(label, name, vec![value.clone(), flags.clone()]);
            }
        }
        matrix.check(format!("{name}(1, 1, 1)"), name, vec![Value::Int(1); 3]);
    }

    for id in [-256, -1, 0, 22, 255, 256, 262, 394, 395, 397, 411, 999] {
        let mut constructor = vec![Value::Int(id), Value::from(&b"name\xff"[..])];
        for (line, pos) in [(None, None), (Some(5), None), (Some(5), Some(12))] {
            constructor.truncate(2);
            constructor.extend(line.map(Value::Int));
            constructor.extend(pos.map(Value::Int));
            let object = instantiate(&matrix.registry, "PhpToken").unwrap();
            let create = format!("new PhpToken({})", literals(&constructor));
            let mut context = TestContext::default();
            let result = call_method(
                &matrix.registry,
                &mut context,
                &object,
                "__construct",
                constructor.clone(),
            )
            .map(|_| Value::Object(object.clone()));
            matrix.record(create.clone(), &create, result, &context);

            let mut is_args = arguments.to_vec();
            is_args.extend([
                Value::Int(id),
                Value::from(&b"name\xff"[..]),
                list([Value::Int(id), Value::Bool(false)]),
                list([Value::Bool(false), Value::Int(id)]),
                list([Value::from("miss"), Value::Int(id)]),
                list([Value::Int(1000), list([Value::Int(id)])]),
            ]);
            for method in ["is", "isIgnorable", "getTokenName", "__toString"] {
                for args in
                    std::iter::once(Vec::new()).chain(is_args.iter().cloned().map(|arg| vec![arg]))
                {
                    let expression = format!("({create})->{method}({})", literals(&args));
                    let mut context = TestContext::default();
                    let result = call_method(&matrix.registry, &mut context, &object, method, args);
                    matrix.record(expression.clone(), &expression, result, &context);
                }
            }
        }
    }

    for value in &arguments {
        for args in [
            vec![value.clone()],
            vec![value.clone(); 2],
            vec![value.clone(); 5],
        ] {
            let object = instantiate(&matrix.registry, "PhpToken").unwrap();
            let expression = format!("new PhpToken({})", literals(&args));
            let mut context = TestContext::default();
            let result = call_method(&matrix.registry, &mut context, &object, "__construct", args)
                .map(|_| Value::Object(object));
            matrix.record(expression.clone(), &expression, result, &context);
        }
    }

    let php = oracle::run_php("tokenizer-api", &matrix.php).expect("PHP 8.5 required");
    oracle::compare("tokenizer API", &matrix.labels, &php, &matrix.results);
    println!("{} function and method calls matched", matrix.labels.len());
}

fn literals(args: &[Value]) -> String {
    args.iter()
        .map(oracle::php_literal)
        .collect::<Vec<_>>()
        .join(", ")
}

fn list<const N: usize>(values: [Value; N]) -> Value {
    let mut array = phera_extension_api::PhpArray::new();
    for (index, value) in values.into_iter().enumerate() {
        array.insert(ArrayKey::Int(index as i64), value);
    }
    Value::from(array)
}

#[test]
fn parse_errors_use_the_parsed_source_location() {
    use phera_extension_api::testing::call;

    let matrix = Matrix::new("tokenizer-locations");
    let examples: &[&[u8]] = &[
        b"<?php\r\n\r\necho ;",
        b"<?php\n/* unfinished",
        b"<?php\n<<<END\n\t text\n \tEND;",
        b"<?php\nif (false) { __halt_compiler(); }",
        b"<?php echo \"unterminated\n",
    ];
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    let mut actual = Vec::new();
    let mut labels = Vec::new();
    for (index, source) in examples.iter().enumerate() {
        let value = Value::from(*source);
        let mut context = TestContext::default();
        let error = call(
            &matrix.registry,
            &mut context,
            "token_get_all",
            vec![value.clone(), Value::Int(1)],
        )
        .unwrap_err();
        let (class, message, code, properties) = error.into_exception_parts().unwrap();
        let file = properties.get(&ArrayKey::str("file")).unwrap().clone();
        let line = properties.get(&ArrayKey::str("line")).unwrap().clone();
        let result = Ok(list([
            Value::from(class),
            Value::from(message),
            code,
            file,
            line,
        ]));
        let source = oracle::php_literal(&value);
        script.push_str(&format!(
            "run('{index}', function () {{
                try {{ token_get_all({source}, TOKEN_PARSE); }}
                catch (Throwable $e) {{
                    return [get_class($e), $e->getMessage(), $e->getCode(),
                            $e->getFile(), $e->getLine()];
                }}
            }});\n"
        ));
        labels.push(format!("token_get_all({source}, TOKEN_PARSE)"));
        actual.push(oracle::encode_case(
            &index.to_string(),
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }
    let php = oracle::run_php("tokenizer-locations", &script).expect("PHP 8.5 required");
    oracle::compare("tokenizer parse locations", &labels, &php, &actual);
}
