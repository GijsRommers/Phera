//! Encoding and decoding against byte sequences from the protobuf
//! encoding guide, and descriptor sets assembled by hand.

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::wire::{WireType, put_tag, put_varint};
use super::{Schema, decode, define, descriptor, encode};

fn array(entries: Vec<(&str, Value)>) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(ArrayKey::str(key), value);
    }
    array
}

fn list(values: Vec<Value>) -> Value {
    let len = values.len();
    Value::from(PhpArray::from_values(values, len))
}

fn field(name: &str, kind: &str) -> Value {
    Value::from(array(vec![
        ("name", Value::from(name)),
        ("type", Value::from(kind)),
    ]))
}

fn field_with(name: &str, kind: &str, extra: Vec<(&str, Value)>) -> Value {
    let mut entries = vec![("name", Value::from(name)), ("type", Value::from(kind))];
    entries.extend(extra);
    Value::from(array(entries))
}

fn message(fields: Vec<(i64, Value)>) -> Value {
    let mut array = PhpArray::new();
    for (number, field) in fields {
        array.insert(ArrayKey::Int(number), field);
    }
    Value::from(array)
}

fn schema(messages: Vec<(&str, Value)>) -> Schema {
    define::parse(&array(messages), &PhpArray::new()).expect("valid schema")
}

/// A compact, ordered rendering of a decoded value.
fn render(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(flag) => flag.to_string(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => format!("{number:?}"),
        Value::Array(array) => {
            let items: Vec<String> = array
                .iter()
                .map(|(key, value)| {
                    let key = match key {
                        ArrayKey::Int(number) => number.to_string(),
                        ArrayKey::Interned(name) => name.to_string_lossy(),
                        ArrayKey::String(name) => name.to_string_lossy(),
                    };
                    format!("{key}:{}", render(&value.deref()))
                })
                .collect();
            format!("{{{}}}", items.join(","))
        }
        other => format!(
            "{:?}",
            String::from_utf8_lossy(other.as_str_bytes().unwrap_or(b"?"))
        ),
    }
}

fn encoded(schema: &Schema, name: &str, data: Vec<(&str, Value)>) -> Vec<u8> {
    let mut out = Vec::new();
    encode(
        schema,
        schema.message_id(name).unwrap(),
        &array(data),
        &mut out,
    )
    .expect("encodes");
    out
}

fn decoded(schema: &Schema, name: &str, bytes: &[u8]) -> String {
    let array = decode(schema, schema.message_id(name).unwrap(), bytes).expect("decodes");
    render(&Value::from(array))
}

#[test]
fn scalars_match_the_encoding_guide() {
    let schema = schema(vec![(
        "t.Scalars",
        message(vec![
            (1, field("i32", "int32")),
            (2, field("text", "string")),
            (3, field("s32", "sint32")),
            (4, field("f32", "fixed32")),
            (5, field("d", "double")),
            (6, field("f", "float")),
            (7, field("flag", "bool")),
            (8, field("u64", "uint64")),
            (9, field("s64", "sint64")),
            (10, field("sf64", "sfixed64")),
            (11, field("raw", "bytes")),
            (12, field("u32", "uint32")),
            (13, field("i64", "int64")),
            (14, field("sf32", "sfixed32")),
            (15, field("f64", "fixed64")),
            (16, field("color", "enum")),
        ]),
    )]);
    let cases: Vec<(&str, Value, &[u8])> = vec![
        ("i32", Value::Int(150), b"\x08\x96\x01"),
        (
            "i32",
            Value::Int(-1),
            b"\x08\xff\xff\xff\xff\xff\xff\xff\xff\xff\x01",
        ),
        ("text", Value::from("testing"), b"\x12\x07testing"),
        ("s32", Value::Int(-1), b"\x18\x01"),
        ("s32", Value::Int(-2), b"\x18\x03"),
        ("f32", Value::Int(1), b"\x25\x01\x00\x00\x00"),
        (
            "d",
            Value::Float(1.0),
            b"\x29\x00\x00\x00\x00\x00\x00\xf0\x3f",
        ),
        ("f", Value::Float(1.5), b"\x35\x00\x00\xc0\x3f"),
        ("flag", Value::Bool(true), b"\x38\x01"),
        (
            "u64",
            Value::from("18446744073709551615"),
            b"\x40\xff\xff\xff\xff\xff\xff\xff\xff\xff\x01",
        ),
        (
            "s64",
            Value::Int(i64::MIN),
            b"\x48\xff\xff\xff\xff\xff\xff\xff\xff\xff\x01",
        ),
        (
            "sf64",
            Value::Int(-2),
            b"\x51\xfe\xff\xff\xff\xff\xff\xff\xff",
        ),
        ("raw", Value::from(&b"\x00\xff"[..]), b"\x5a\x02\x00\xff"),
        (
            "u32",
            Value::Int(4_294_967_295),
            b"\x60\xff\xff\xff\xff\x0f",
        ),
        (
            "i64",
            Value::Int(-1),
            b"\x68\xff\xff\xff\xff\xff\xff\xff\xff\xff\x01",
        ),
        ("sf32", Value::Int(-1), b"\x75\xff\xff\xff\xff"),
        (
            "f64",
            Value::Int(1),
            b"\x79\x01\x00\x00\x00\x00\x00\x00\x00",
        ),
        ("color", Value::Int(2), b"\x80\x01\x02"),
    ];
    for (name, value, bytes) in cases {
        let out = encoded(&schema, "t.Scalars", vec![(name, value.clone())]);
        assert_eq!(out, bytes, "encoding {name} = {}", render(&value));
        let array = decode(&schema, schema.message_id("t.Scalars").unwrap(), bytes).unwrap();
        let back = array.get(&ArrayKey::str(name)).unwrap();
        let expected = match (name, &value) {
            // Above PHP_INT_MAX: the two's complement int.
            ("u64", _) => Value::Int(-1),
            (_, Value::InternedStr(_) | Value::String(_)) => value.clone(),
            _ => value.clone(),
        };
        assert_eq!(render(back), render(&expected), "decoding {name}");
    }
}

#[test]
fn proto3_defaults_are_left_out_and_filled_back_in() {
    let schema = schema(vec![(
        "t.M",
        message(vec![
            (1, field("id", "int64")),
            (2, field("name", "string")),
            (3, field("on", "bool")),
            (4, field("ratio", "double")),
            (
                5,
                field_with("tags", "string", vec![("repeated", Value::Bool(true))]),
            ),
            (
                6,
                field_with("note", "string", vec![("optional", Value::Bool(true))]),
            ),
            (7, field("child", "t.M")),
        ]),
    )]);
    let defaults = vec![
        ("id", Value::Int(0)),
        ("name", Value::from("")),
        ("on", Value::Bool(false)),
        ("ratio", Value::Float(0.0)),
        ("tags", list(vec![])),
    ];
    assert!(encoded(&schema, "t.M", defaults).is_empty());
    assert_eq!(
        decoded(&schema, "t.M", b""),
        r#"{id:0,name:"",on:false,ratio:0.0,tags:{}}"#
    );
    // A field with presence is written even at its default.
    let out = encoded(&schema, "t.M", vec![("note", Value::from(""))]);
    assert_eq!(out, b"\x32\x00");
    assert_eq!(
        decoded(&schema, "t.M", &out),
        r#"{id:0,name:"",on:false,ratio:0.0,tags:{},note:""}"#
    );
    // Null means unset.
    assert!(
        encoded(
            &schema,
            "t.M",
            vec![("child", Value::Null), ("id", Value::Null)]
        )
        .is_empty()
    );
    let out = encoded(
        &schema,
        "t.M",
        vec![("child", Value::from(PhpArray::new()))],
    );
    assert_eq!(out, b"\x3a\x00");
}

#[test]
fn repeated_numbers_are_packed_unless_asked_and_read_either_way() {
    let schema = schema(vec![(
        "t.R",
        message(vec![
            (
                4,
                field_with("packed", "int32", vec![("repeated", Value::Bool(true))]),
            ),
            (
                5,
                field_with(
                    "loose",
                    "int32",
                    vec![
                        ("repeated", Value::Bool(true)),
                        ("packed", Value::Bool(false)),
                    ],
                ),
            ),
            (
                6,
                field_with("words", "string", vec![("repeated", Value::Bool(true))]),
            ),
        ]),
    )]);
    let numbers = || list(vec![Value::Int(1), Value::Int(2), Value::Int(300)]);
    let out = encoded(&schema, "t.R", vec![("packed", numbers())]);
    assert_eq!(out, b"\x22\x04\x01\x02\xac\x02");
    let out = encoded(&schema, "t.R", vec![("loose", numbers())]);
    assert_eq!(out, b"\x28\x01\x28\x02\x28\xac\x02");
    // Each decoder accepts the other encoding.
    assert_eq!(
        decoded(&schema, "t.R", b"\x20\x01\x20\x02\x2a\x02\x03\x04"),
        "{packed:{0:1,1:2},loose:{0:3,1:4},words:{}}"
    );
    let out = encoded(
        &schema,
        "t.R",
        vec![("words", list(vec![Value::from("a"), Value::from("bc")]))],
    );
    assert_eq!(out, b"\x32\x01a\x32\x02bc");
    assert_eq!(
        decoded(&schema, "t.R", &out),
        r#"{packed:{},loose:{},words:{0:"a",1:"bc"}}"#
    );
}

#[test]
fn nested_messages_round_trip_through_the_length_backpatch() {
    let schema = schema(vec![
        (
            "t.Outer",
            message(vec![
                (1, field("inner", "Inner")),
                (
                    2,
                    field_with("items", "t.Inner", vec![("repeated", Value::Bool(true))]),
                ),
                (3, field("id", "int32")),
            ]),
        ),
        ("t.Inner", message(vec![(1, field("text", "string"))])),
        ("Inner", message(vec![(1, field("text", "string"))])),
    ]);
    // Relative names resolve inside out: "Inner" from t.Outer is t.Inner.
    let long = "x".repeat(300);
    let inner = |text: &str| Value::from(array(vec![("text", Value::from(text))]));
    let out = encoded(
        &schema,
        "t.Outer",
        vec![
            ("inner", inner(&long)),
            ("items", list(vec![inner("a"), inner(&long)])),
            ("id", Value::Int(7)),
        ],
    );
    // 300 bytes of text need a two-byte length, and so does its message.
    assert_eq!(&out[..5], b"\x0a\xaf\x02\x0a\xac");
    let back = decode(&schema, schema.message_id("t.Outer").unwrap(), &out).unwrap();
    let back = Value::from(back);
    let expected =
        format!(r#"{{inner:{{text:"{long}"}},items:{{0:{{text:"a"}},1:{{text:"{long}"}}}},id:7}}"#);
    assert_eq!(render(&back), expected);
}

#[test]
fn a_message_field_seen_twice_is_merged() {
    let schema = schema(vec![
        ("t.Outer", message(vec![(1, field("inner", "t.Inner"))])),
        (
            "t.Inner",
            message(vec![
                (1, field("a", "int32")),
                (2, field("b", "int32")),
                (
                    3,
                    field_with("list", "int32", vec![("repeated", Value::Bool(true))]),
                ),
            ]),
        ),
    ]);
    // inner{a:1, list:[1]} then inner{b:2, list:[2]}
    let bytes = b"\x0a\x04\x08\x01\x18\x01\x0a\x04\x10\x02\x18\x02";
    assert_eq!(
        decoded(&schema, "t.Outer", bytes),
        "{inner:{a:1,b:2,list:{0:1,1:2}}}"
    );
}

#[test]
fn maps_use_entry_messages() {
    let schema = schema(vec![
        (
            "t.Inventory",
            message(vec![
                (
                    1,
                    field_with(
                        "stock",
                        "map",
                        vec![
                            ("key", Value::from("string")),
                            ("value", Value::from("int32")),
                        ],
                    ),
                ),
                (
                    2,
                    field_with(
                        "items",
                        "map",
                        vec![
                            ("key", Value::from("int64")),
                            ("value", Value::from("t.Item")),
                        ],
                    ),
                ),
            ]),
        ),
        ("t.Item", message(vec![(1, field("name", "string"))])),
    ]);
    let stock = array(vec![("apple", Value::Int(3)), ("pear", Value::Int(0))]);
    let mut items = PhpArray::new();
    items.insert(
        ArrayKey::Int(42),
        Value::from(array(vec![("name", Value::from("bolt"))])),
    );
    let out = encoded(
        &schema,
        "t.Inventory",
        vec![("stock", Value::from(stock)), ("items", Value::from(items))],
    );
    // Entries carry their value even at the default: pear => 0.
    assert_eq!(
        &out[..21],
        b"\x0a\x09\x0a\x05apple\x10\x03\x0a\x08\x0a\x04pear\x10\x00"
    );
    assert_eq!(
        decoded(&schema, "t.Inventory", &out),
        r#"{stock:{apple:3,pear:0},items:{42:{name:"bolt"}}}"#
    );
    // A missing key or value is its default.
    assert_eq!(
        decoded(&schema, "t.Inventory", b"\x0a\x00\x12\x02\x08\x05"),
        "{stock:{:0},items:{5:{}}}"
    );
}

#[test]
fn mistakes_are_reported_with_their_place() {
    let schema = schema(vec![
        ("t.Outer", message(vec![(1, field("inner", "t.Inner"))])),
        (
            "t.Inner",
            message(vec![
                (1, field("count", "int32")),
                (2, field("text", "string")),
            ]),
        ),
    ]);
    let id = schema.message_id("t.Outer").unwrap();
    let bad = array(vec![(
        "inner",
        Value::from(array(vec![("count", Value::from(PhpArray::new()))])),
    )]);
    let error = encode(&schema, id, &bad, &mut Vec::new()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "t.Outer.inner.count: expected an integer, got array"
    );
    let bad = array(vec![(
        "inner",
        Value::from(array(vec![("cuont", Value::Int(1))])),
    )]);
    let error = encode(&schema, id, &bad, &mut Vec::new()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "t.Outer.inner: message t.Inner has no field \"cuont\""
    );
    let bad = array(vec![(
        "inner",
        Value::from(array(vec![("count", Value::Int(1 << 40))])),
    )]);
    let error = encode(&schema, id, &bad, &mut Vec::new()).unwrap_err();
    assert!(error.to_string().contains("out of range"), "{error}");
    let bad = array(vec![(
        "inner",
        Value::from(array(vec![("text", Value::from(&b"\xff"[..]))])),
    )]);
    let error = encode(&schema, id, &bad, &mut Vec::new()).unwrap_err();
    assert!(error.to_string().contains("UTF-8"), "{error}");
    // Unknown fields are skipped; truncated input is an error.
    assert_eq!(
        decoded(&schema, "t.Inner", b"\x18\x05\x08\x02"),
        r#"{count:2,text:""}"#
    );
    let error = decode(
        &schema,
        schema.message_id("t.Inner").unwrap(),
        b"\x12\x05ab",
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "t.Inner: unexpected end of message");
}

#[test]
fn schema_definitions_are_checked() {
    let parse =
        |messages: Vec<(&str, Value)>| define::parse(&array(messages), &PhpArray::new()).map(drop);
    let error = parse(vec![("t.A", message(vec![(1, field("x", "t.Missing"))]))]).unwrap_err();
    assert_eq!(error, "field t.A.x: unknown type t.Missing");
    let error = parse(vec![(
        "t.A",
        message(vec![(1, field("x", "int32")), (2, field("x", "int32"))]),
    )])
    .unwrap_err();
    assert_eq!(error, "field t.A.x is defined twice");
    let error = parse(vec![("t.A", message(vec![(0, field("x", "int32"))]))]).unwrap_err();
    assert_eq!(error, "field t.A.x has invalid number 0");
}

#[test]
fn field_numbers_above_the_direct_table_are_found_by_search() {
    let schema = schema(vec![(
        "t.Wide",
        message(vec![
            (1, field("low", "int32")),
            (64, field("edge", "int32")),
            (65, field("above", "int32")),
            (536_870_911, field("max", "int32")),
        ]),
    )]);
    let out = encoded(
        &schema,
        "t.Wide",
        vec![
            ("low", Value::Int(1)),
            ("edge", Value::Int(2)),
            ("above", Value::Int(3)),
            ("max", Value::Int(4)),
        ],
    );
    assert_eq!(
        decoded(&schema, "t.Wide", &out),
        "{low:1,edge:2,above:3,max:4}"
    );
}

// --- descriptor sets ----------------------------------------------------

fn len_field(out: &mut Vec<u8>, number: u32, bytes: &[u8]) {
    put_tag(out, number, WireType::Len);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn varint_field(out: &mut Vec<u8>, number: u32, value: u64) {
    put_tag(out, number, WireType::Varint);
    put_varint(out, value);
}

/// A FieldDescriptorProto.
fn fd(name: &str, number: u64, label: u64, kind: u64, type_name: Option<&str>) -> Vec<u8> {
    let mut out = Vec::new();
    len_field(&mut out, 1, name.as_bytes());
    varint_field(&mut out, 3, number);
    varint_field(&mut out, 4, label);
    varint_field(&mut out, 5, kind);
    if let Some(type_name) = type_name {
        len_field(&mut out, 6, type_name.as_bytes());
    }
    out
}

fn named(name: &str, parts: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    len_field(&mut out, 1, name.as_bytes());
    for (number, bytes) in parts {
        len_field(&mut out, *number, bytes);
    }
    out
}

fn method(name: &str, input: &str, output: &str, server_streaming: bool) -> Vec<u8> {
    let mut out = named(
        name,
        &[
            (2, input.as_bytes().to_vec()),
            (3, output.as_bytes().to_vec()),
        ],
    );
    if server_streaming {
        varint_field(&mut out, 6, 1);
    }
    out
}

/// test.proto, as protoc would describe it:
///
/// ```proto
/// syntax = "proto3";
/// package shop;
/// enum Color { RED = 0; BLUE = 1; }
/// message Item {
///   int32 id = 1;
///   repeated int32 sizes = 2;
///   Price price = 3;
///   map<string, int64> stock = 4;
///   optional string note = 5;
///   Color color = 6;
///   repeated int32 loose = 7 [packed = false];
///   message Price { int64 cents = 1; }
/// }
/// service Shop {
///   rpc Get(Item) returns (Item.Price);
///   rpc Watch(Item) returns (stream Item);
/// }
/// ```
fn shop_descriptor_set() -> Vec<u8> {
    let mut proto3_optional = fd("note", 5, 1, 9, None);
    varint_field(&mut proto3_optional, 9, 0); // oneof_index of the synthetic oneof
    varint_field(&mut proto3_optional, 17, 1);
    let mut loose = fd("loose", 7, 3, 5, None);
    let mut packed_false = Vec::new();
    varint_field(&mut packed_false, 2, 0);
    len_field(&mut loose, 8, &packed_false);
    let mut map_entry_options = Vec::new();
    varint_field(&mut map_entry_options, 7, 1);
    let stock_entry = named(
        "StockEntry",
        &[
            (2, fd("key", 1, 1, 9, None)),
            (2, fd("value", 2, 1, 3, None)),
            (7, map_entry_options),
        ],
    );
    let price = named("Price", &[(2, fd("cents", 1, 1, 3, None))]);
    let item = named(
        "Item",
        &[
            (2, fd("id", 1, 1, 5, None)),
            (2, fd("sizes", 2, 3, 5, None)),
            (2, fd("price", 3, 1, 11, Some(".shop.Item.Price"))),
            (2, fd("stock", 4, 3, 11, Some(".shop.Item.StockEntry"))),
            (2, proto3_optional),
            (2, fd("color", 6, 1, 14, Some(".shop.Color"))),
            (2, loose),
            (3, price),
            (3, stock_entry),
        ],
    );
    let color = named("Color", &[]);
    let service = named(
        "Shop",
        &[
            (2, method("Get", ".shop.Item", ".shop.Item.Price", false)),
            (2, method("Watch", ".shop.Item", ".shop.Item", true)),
        ],
    );
    let mut file = named("shop.proto", &[]);
    len_field(&mut file, 2, b"shop");
    len_field(&mut file, 4, &item);
    len_field(&mut file, 5, &color);
    len_field(&mut file, 6, &service);
    len_field(&mut file, 12, b"proto3");
    let mut set = Vec::new();
    len_field(&mut set, 1, &file);
    set
}

#[test]
fn descriptor_sets_from_protoc_are_read() {
    let schema = descriptor::parse(&shop_descriptor_set()).expect("valid descriptor set");
    let names: Vec<&str> = schema.message_names().collect();
    assert_eq!(names, ["shop.Item", "shop.Item.Price"]);
    let get = schema.method("/shop.Shop/Get").expect("method");
    assert_eq!(&*schema.message(get.input).name, "shop.Item");
    assert_eq!(&*schema.message(get.output).name, "shop.Item.Price");
    assert!(schema.method("/shop.Shop/Watch").unwrap().server_streaming);

    let mut stock = PhpArray::new();
    stock.insert(ArrayKey::str("bolt"), Value::Int(12));
    let out = encoded(
        &schema,
        "shop.Item",
        vec![
            ("id", Value::Int(1)),
            ("sizes", list(vec![Value::Int(3), Value::Int(4)])),
            (
                "price",
                Value::from(array(vec![("cents", Value::Int(250))])),
            ),
            ("stock", Value::from(stock)),
            ("note", Value::from("")),
            ("color", Value::Int(1)),
            ("loose", list(vec![Value::Int(5), Value::Int(6)])),
        ],
    );
    // proto3 packs `sizes`; `[packed = false]` keeps `loose` one tag each.
    let mut expected = b"\x08\x01\x12\x02\x03\x04\x1a\x03\x08\xfa\x01".to_vec();
    expected.extend_from_slice(b"\x22\x08\x0a\x04bolt\x10\x0c\x2a\x00\x30\x01\x38\x05\x38\x06");
    assert_eq!(out, expected);
    assert_eq!(
        decoded(&schema, "shop.Item", &out),
        r#"{id:1,sizes:{0:3,1:4},price:{cents:250},stock:{bolt:12},note:"",color:1,loose:{0:5,1:6}}"#
    );
    // Without the optional note and the message, they stay absent.
    assert_eq!(
        decoded(&schema, "shop.Item", b""),
        "{id:0,sizes:{},stock:{},color:0,loose:{}}"
    );
}

#[test]
fn proto2_fields_have_presence_and_are_not_packed() {
    let mut file = named("old.proto", &[]);
    len_field(&mut file, 2, b"old");
    len_field(
        &mut file,
        4,
        &named(
            "M",
            &[(2, fd("id", 1, 1, 5, None)), (2, fd("ids", 2, 3, 5, None))],
        ),
    );
    let mut set = Vec::new();
    len_field(&mut set, 1, &file);
    let schema = descriptor::parse(&set).unwrap();
    let out = encoded(
        &schema,
        "old.M",
        vec![
            ("id", Value::Int(0)),
            ("ids", list(vec![Value::Int(1), Value::Int(2)])),
        ],
    );
    assert_eq!(out, b"\x08\x00\x10\x01\x10\x02");
    assert_eq!(decoded(&schema, "old.M", b""), "{ids:{}}");
}

#[test]
fn broken_descriptor_sets_are_rejected() {
    assert!(descriptor::parse(b"\x0a\x05ab").is_err());
    let mut file = named("x.proto", &[]);
    len_field(&mut file, 4, &named("M", &[(2, fd("g", 1, 1, 10, None))]));
    let mut set = Vec::new();
    len_field(&mut set, 1, &file);
    let error = descriptor::parse(&set).unwrap_err();
    assert!(error.contains("groups"), "{error}");
}
