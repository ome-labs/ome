//! A call's arguments split by its function signature, so a reviewer sees which value is which parameter. Aztec
//! encodes every parameter as a run of fields: one per `Field`, integer or `bool`, the members of a struct or tuple in
//! order, `N` per array of `N` and per `str<N>`. The signature names only types, so a struct shows as its tuple.

use ome_wire::types::WireField;

use crate::Error;

/// One parameter of a call: its type as the signature writes it, and its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Argument {
    /// The type, e.g. `(Field)`, `u128` or `[(Field,Field);8]`.
    pub kind: String,
    /// Its encoded fields, in order.
    pub fields: Vec<WireField>,
}

/// Splits `args` by the parameters of `signature` (`name(type,...)`).
///
/// # Errors
/// [`Error::UndecodableArguments`] for a type this decoder does not know, or when the parameters do not account for
/// exactly the arguments given.
pub fn arguments(signature: &str, args: &[WireField]) -> Result<Vec<Argument>, Error> {
    let params = signature
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .ok_or(Error::UndecodableArguments("not a function signature"))?;
    let mut rest = args;
    let mut out = Vec::new();
    for kind in split_top_level(params) {
        let count = width(kind)?;
        if count > rest.len() {
            return Err(Error::UndecodableArguments(
                "fewer arguments than parameters",
            ));
        }
        let (fields, tail) = rest.split_at(count);
        out.push(Argument {
            kind: kind.to_owned(),
            fields: fields.to_vec(),
        });
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(Error::UndecodableArguments(
            "more arguments than parameters",
        ));
    }
    Ok(out)
}

/// The fields a type takes.
fn width(kind: &str) -> Result<usize, Error> {
    let unknown = Error::UndecodableArguments("unknown parameter type");
    if kind == "Field" || kind == "bool" || is_integer(kind) {
        return Ok(1);
    }
    if let Some(inner) = kind.strip_prefix('(').and_then(|k| k.strip_suffix(')')) {
        return split_top_level(inner)
            .into_iter()
            .try_fold(0usize, |sum, member| Ok(sum + width(member)?));
    }
    if let Some(inner) = kind.strip_prefix('[').and_then(|k| k.strip_suffix(']')) {
        let (element, length) = inner.rsplit_once(';').ok_or(unknown)?;
        let length: usize = length.parse().map_err(|_| unknown_length())?;
        return width(element)?
            .checked_mul(length)
            .ok_or_else(unknown_length);
    }
    if let Some(length) = kind.strip_prefix("str<").and_then(|k| k.strip_suffix('>')) {
        return length.parse().map_err(|_| unknown_length());
    }
    Err(unknown)
}

fn unknown_length() -> Error {
    Error::UndecodableArguments("bad array or string length")
}

fn is_integer(kind: &str) -> bool {
    kind.strip_prefix('u')
        .or_else(|| kind.strip_prefix('i'))
        .is_some_and(|bits| !bits.is_empty() && bits.bytes().all(|b| b.is_ascii_digit()))
}

/// Splits a comma-separated list at depth zero; an empty list has no items.
fn split_top_level(list: &str) -> Vec<&str> {
    if list.is_empty() {
        return Vec::new();
    }
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in list.char_indices() {
        match c {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                items.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&list[start..]);
    items
}
