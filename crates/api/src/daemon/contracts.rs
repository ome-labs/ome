//! Contracts a vault calls: their functions with full ABI types, typed argument values, decoded calls as reviews show
//! them, and the contracts this member can call.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Hex32, HexBytes};

/// A Noir ABI type, complete enough to draw a form for any parameter and to check a value against it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AbiType {
    /// A field element.
    Field,
    /// `true` or `false`.
    Boolean,
    /// An integer of `width` bits.
    Integer {
        /// Signed.
        signed: bool,
        /// Bits.
        width: u32,
    },
    /// A fixed-length array.
    Array {
        /// Elements.
        length: u32,
        /// Element type.
        item: Box<AbiType>,
    },
    /// A fixed-length string.
    String {
        /// Most characters.
        length: u32,
    },
    /// A struct.
    Struct {
        /// Its Noir path, e.g. `aztec::protocol_types::address::aztec_address::AztecAddress`.
        path: String,
        /// Its fields, in order.
        fields: Vec<AbiField>,
        /// Set for structs that are one value to a person (an address, a selector), so forms and reviews show them as
        /// one value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        known: Option<KnownType>,
    },
    /// A tuple.
    Tuple {
        /// Its items, in order.
        items: Vec<AbiType>,
    },
}

/// A struct that is one value to a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum KnownType {
    /// An Aztec address: `0x` and 64 hex characters.
    AztecAddress,
    /// An Ethereum address: `0x` and 40 hex characters.
    EthAddress,
    /// A function selector: `0x` and 8 hex characters.
    FunctionSelector,
}

/// A struct field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AbiField {
    /// Name.
    pub name: String,
    /// Type.
    #[serde(rename = "type")]
    pub ty: AbiType,
}

/// An argument value, checked against its [`AbiType`]: text for fields, integers (decimal, or `0x` hex), addresses and
/// strings; a boolean; a list for arrays and tuples; an object for structs. A known struct takes text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum AbiValue {
    /// A boolean.
    Bool(bool),
    /// A field, integer, address or string.
    Text(String),
    /// Array or tuple items.
    List(Vec<AbiValue>),
    /// Struct fields by name.
    Object(BTreeMap<String, AbiValue>),
}

/// A function parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FunctionParam {
    /// Name.
    pub name: String,
    /// Type.
    #[serde(rename = "type")]
    pub ty: AbiType,
}

/// Where a function runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    /// Private.
    Private,
    /// Public.
    Public,
    /// Utility (unconstrained).
    Utility,
}

/// A function of a contract, for building `calls` intents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractFunction {
    /// Name.
    pub name: String,
    /// Selector, `0x` and 8 hex characters.
    pub selector: String,
    /// `private`, `public` or `utility`.
    pub kind: FunctionKind,
    /// Static.
    pub is_static: bool,
    /// Parameters.
    pub params: Vec<FunctionParam>,
    /// Return types.
    pub returns: Vec<AbiType>,
}

/// Params of `contracts.functions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractFunctionsParams {
    /// Contract.
    pub contract: Hex32,
    /// Include utility (read-only) functions, which a proposal can't call. Off when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub include_utility: Option<bool>,
}

/// One argument of a decoded call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DecodedArg {
    /// Parameter name; `arg0`, `arg1`… when the call couldn't be decoded.
    pub name: String,
    /// Parameter type; a field when the call couldn't be decoded.
    #[serde(rename = "type")]
    pub ty: AbiType,
    /// The value.
    pub value: AbiValue,
    /// How a person reads it, when the daemon knows better than the raw value: `bob (alias)`, `75.5 OMET`.
    pub display: Option<String>,
}

/// Where this member's knowledge of a contract comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ContractSource {
    /// Registered by the member, or fetched for a proposal.
    Registered,
    /// A token the vault tracks.
    Token,
    /// The vault itself.
    Vault,
}

/// A contract this member can call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractView {
    /// Address.
    pub address: Hex32,
    /// The artifact's name.
    pub name: String,
    /// Contract class.
    pub class_id: Hex32,
    /// Where it comes from.
    pub source: ContractSource,
    /// Functions a proposal can call.
    pub functions: u32,
}

/// What an unpublished contract instance derives from, to register it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContractInstanceInput {
    /// Deployment salt.
    pub salt: Hex32,
    /// Deployer, zero for a universal deployment.
    pub deployer: Hex32,
    /// Initialization hash.
    pub initialization_hash: Hex32,
    /// Hash of the immutable values the contract was deployed with.
    pub immutables_hash: Hex32,
    /// Public keys, serialized as Aztec does.
    pub public_keys: HexBytes,
}

/// Params of `contracts.register`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ContractRegisterParams {
    /// The artifact, as JSON text.
    pub artifact: String,
    /// The contract's address.
    pub address: Hex32,
    /// Needed only when the node has no published instance at `address`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<ContractInstanceInput>,
}

/// What the daemon accepts, so clients don't hard-code it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// Calls per proposal besides the fee payer's.
    pub max_calls: u32,
    /// Private authwits a proposal grants.
    pub max_authwits: u32,
    /// Bytes of a proposal's summary, which carries its intent.
    pub max_summary_bytes: u32,
    /// Bytes of a contract artifact, uncompressed.
    pub max_artifact_bytes: u32,
    /// Characters of a memo or rejection reason.
    pub memo_chars: u32,
    /// Fewest hours a proposal can last.
    pub min_expiry_hours: u32,
    /// Most hours a proposal can last.
    pub max_expiry_hours: u32,
}
