//! Typed call arguments on screen and on the command line: a type as people read it (`u128`, `AztecAddress`,
//! `[Field; 3]`), a decoded argument as `name=value`, and an argument typed at the prompt as a value.

use ome_api::daemon::{AbiType, AbiValue, DecodedArg, KnownType};

use super::format;

/// `Field`, `bool`, `u128`, `i8`, `[Field; 3]`, `str<31>`, `AztecAddress`, `Order`, `(u8, bool)`.
pub(crate) fn type_name(ty: &AbiType) -> String {
    match ty {
        AbiType::Field => "Field".into(),
        AbiType::Boolean => "bool".into(),
        AbiType::Integer { signed, width } => format!("{}{width}", if *signed { "i" } else { "u" }),
        AbiType::Array { length, item } => format!("[{}; {length}]", type_name(item)),
        AbiType::String { length } => format!("str<{length}>"),
        AbiType::Struct { path, known, .. } => match known {
            Some(KnownType::AztecAddress) => "AztecAddress".into(),
            Some(KnownType::EthAddress) => "EthAddress".into(),
            Some(KnownType::FunctionSelector) => "FunctionSelector".into(),
            None => path.rsplit("::").next().unwrap_or(path).to_owned(),
        },
        AbiType::Tuple { items } => format!(
            "({})",
            items.iter().map(type_name).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// A value on one line: text as it is (hex shortened by [`format::arg`]), lists and structs as compact JSON.
pub(crate) fn value(value: &AbiValue, verbose: bool) -> String {
    match value {
        AbiValue::Text(text) => format::arg(text, verbose),
        AbiValue::Bool(flag) => flag.to_string(),
        AbiValue::List(_) | AbiValue::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Each argument as `name=value`, preferring what the daemon says it reads as (`bob (alias)`, `75.5 OMET`).
pub(crate) fn decoded(args: &[DecodedArg], verbose: bool) -> Vec<String> {
    args.iter()
        .map(|arg| {
            let shown = arg
                .display
                .clone()
                .unwrap_or_else(|| value(&arg.value, verbose));
            format!("{}={shown}", arg.name)
        })
        .collect()
}

/// An argument as typed: JSON for lists, structs and booleans (`[1,2]`, `{"x":1}`, `true`); anything else is text.
pub(crate) fn parse(text: &str) -> AbiValue {
    let trimmed = text.trim();
    let json =
        trimmed.starts_with('[') || trimmed.starts_with('{') || matches!(trimmed, "true" | "false");
    if json && let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return from_json(value);
    }
    AbiValue::Text(text.to_owned())
}

/// JSON as a value; numbers become text, so `[1, 2]` reads as people type it.
fn from_json(json: serde_json::Value) -> AbiValue {
    use serde_json::Value;
    match json {
        Value::Bool(flag) => AbiValue::Bool(flag),
        Value::Array(items) => AbiValue::List(items.into_iter().map(from_json).collect()),
        Value::Object(fields) => {
            AbiValue::Object(fields.into_iter().map(|(k, v)| (k, from_json(v))).collect())
        }
        Value::String(text) => AbiValue::Text(text),
        Value::Number(number) => AbiValue::Text(number.to_string()),
        Value::Null => AbiValue::Text(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn types_read_as_noir_writes_them() {
        let address = AbiType::Struct {
            path: "aztec::protocol_types::address::aztec_address::AztecAddress".into(),
            fields: vec![],
            known: Some(KnownType::AztecAddress),
        };
        assert_eq!(type_name(&address), "AztecAddress");
        assert_eq!(
            type_name(&AbiType::Array {
                length: 3,
                item: Box::new(AbiType::Integer {
                    signed: false,
                    width: 128
                })
            }),
            "[u128; 3]"
        );
        assert_eq!(
            type_name(&AbiType::Tuple {
                items: vec![AbiType::Field, AbiType::Boolean]
            }),
            "(Field, bool)"
        );
    }

    #[test]
    fn typed_arguments_parse_as_json_only_when_they_look_like_it() {
        assert_eq!(parse("true"), AbiValue::Bool(true));
        assert_eq!(
            parse("[1,\"2\"]"),
            AbiValue::List(vec![AbiValue::Text("1".into()), AbiValue::Text("2".into())])
        );
        assert_eq!(
            parse(r#"{"x":"1"}"#),
            AbiValue::Object(BTreeMap::from([("x".into(), AbiValue::Text("1".into()))]))
        );
        assert_eq!(parse("250"), AbiValue::Text("250".into()));
        assert_eq!(parse("[oops"), AbiValue::Text("[oops".into()));
    }
}
