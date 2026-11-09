//! One resolver for every reference, so people can say what they mean: `0x…`; `accounts:<name>` and
//! `contracts:<name>` from `aztec-wallet`'s alias store; `vault:<name>`; `contact:<label>`; or a bare name, which
//! is looked up in all of them. A token is its symbol, a proposal a unique prefix of its id. When a bare name matches
//! more than one thing, a terminal asks; otherwise the error names the typed forms.

use ome_api::{
    Empty, Hex32,
    daemon::{
        AztecAlias, AztecAliases, CommitteeMember, Contact, ContactsList, DeviceView, DevicesList,
        ProposalView, ProposalsList, ServiceStatus, SignerUnlock, StatusGet, TokenInfo, TokensList,
        UnlockParams, VaultList, VaultSummary,
    },
    keeper::VaultParams,
};

use crate::{
    context::Ctx,
    error::{Error, Result},
    secret,
    ui::{describe, format, prompt, steps::Step},
};

/// Something a reference could mean, and how to name it unambiguously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate<T> {
    pub(crate) value: T,
    /// What it is, for the person: "bob (contact)".
    pub(crate) label: String,
    /// The typed form that names only it: `contact:bob`.
    pub(crate) form: String,
}

/// Picks the one candidate, asks in a terminal when there are several, or fails naming them.
fn pick<T: Clone + Eq>(ctx: &Ctx, input: &str, mut candidates: Vec<Candidate<T>>) -> Result<T> {
    match candidates.len() {
        0 => Err(Error::usage(format!("Nothing is called {input:?}."))),
        1 => Ok(candidates.remove(0).value),
        _ if ctx.interactive => prompt::select(
            ctx,
            input,
            &format!("{input:?} could mean several things. Which one?"),
            candidates
                .into_iter()
                .map(|c| (c.value, c.label, c.form))
                .collect(),
        ),
        _ => Err(
            Error::usage(format!("{input:?} could mean several things.")).hint(format!(
                "Name one: {}.",
                candidates
                    .iter()
                    .map(|c| c.form.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        ),
    }
}

// Vaults.

/// The vaults `input` names: an address or its prefix, an alias, or a name, optionally as `vault:<name>`.
pub(crate) fn match_vaults<'a>(vaults: &'a [VaultSummary], input: &str) -> Vec<&'a VaultSummary> {
    let input = input.strip_prefix("vault:").unwrap_or(input).trim();
    if let Ok(address) = input.parse::<Hex32>() {
        return vaults.iter().filter(|v| v.address == address).collect();
    }
    let lower = input.to_lowercase();
    let named: Vec<_> = vaults
        .iter()
        .filter(|v| {
            v.alias
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(input))
                || v.name.to_lowercase() == lower
        })
        .collect();
    if !named.is_empty() {
        return named;
    }
    hex_prefix(input)
        .map(|prefix| {
            vaults
                .iter()
                .filter(|v| v.address.to_string()[2..].starts_with(&prefix))
                .collect()
        })
        .unwrap_or_default()
}

/// `0x4f2a` or `4f2a` → `4f2a`, for at least four hex digits.
fn hex_prefix(input: &str) -> Option<String> {
    let digits = input.strip_prefix("0x").unwrap_or(input).to_lowercase();
    (digits.len() >= 4 && digits.bytes().all(|b| b.is_ascii_hexdigit())).then_some(digits)
}

fn vault_form(v: &VaultSummary) -> String {
    v.alias
        .as_ref()
        .map_or_else(|| format::short(&v.address), |a| format!("vault:{a}"))
}

/// The member's vaults. After the daemon restarts, its keeper and vaults start with the first unlock (the keeper's
/// key is sealed under the signer's): when they have not, this opens the signer for a moment to start them.
pub(crate) async fn vaults(ctx: &Ctx) -> Result<Vec<VaultSummary>> {
    let client = ctx.client().await?;
    let vaults = client.call::<VaultList>(&Empty {}).await?;
    if !vaults.is_empty() {
        return Ok(vaults);
    }
    let status = client.call::<StatusGet>(&Empty {}).await?;
    if status.profile.is_none() || status.keeper.status != ServiceStatus::Stopped {
        return Ok(vaults);
    }
    ctx.info("Your keeper starts when the signer opens.");
    let secret = secret::for_unlock(ctx, false)
        .map_err(|e| e.hint(format!("Unlock it first: `{}`.", ctx.ome("unlock"))))?;
    let step = Step::start(ctx, "Opening the signer to start your keeper");
    client
        .call::<SignerUnlock>(&UnlockParams {
            secret,
            ttl_secs: 1,
        })
        .await?;
    step.done("Keeper started");
    Ok(client.call::<VaultList>(&Empty {}).await?)
}

/// The vault a command acts on: `--vault`, else the chosen vault, else the only one.
pub(crate) async fn vault(ctx: &Ctx) -> Result<VaultSummary> {
    let vaults = vaults(ctx).await?;
    vault_in(ctx, &vaults, ctx.vault.as_deref())
}

pub(crate) fn vault_in(
    ctx: &Ctx,
    vaults: &[VaultSummary],
    input: Option<&str>,
) -> Result<VaultSummary> {
    if let Some(input) = input {
        let matches = match_vaults(vaults, input);
        if matches.is_empty() {
            return Err(Error::usage(format!("No vault matches {input:?}."))
                .hint(format!("See yours with `{}`.", ctx.ome("vault list"))));
        }
        let candidates = matches
            .iter()
            .map(|v| Candidate {
                value: v.address,
                label: v.name.clone(),
                form: vault_form(v),
            })
            .collect();
        let address = pick(ctx, input, candidates)?;
        return Ok(vaults
            .iter()
            .find(|v| v.address == address)
            .cloned()
            .unwrap_or_else(|| matches[0].clone()));
    }
    if let Some(chosen) = vaults.iter().find(|v| v.is_default) {
        return Ok(chosen.clone());
    }
    match vaults {
        [] => Err(Error::usage("You have no vault yet.").hint(format!(
            "Create one with `{}`, or join one with `{}`.",
            ctx.ome("vault create"),
            ctx.ome("vault join <invite>")
        ))),
        [only] => Ok(only.clone()),
        _ if ctx.interactive => {
            let address = prompt::select(
                ctx,
                "--vault",
                "Which vault?",
                vaults
                    .iter()
                    .map(|v| (v.address, v.name.clone(), vault_form(v)))
                    .collect(),
            )?;
            Ok(vaults
                .iter()
                .find(|v| v.address == address)
                .cloned()
                .unwrap_or_else(|| vaults[0].clone()))
        }
        _ => Err(
            Error::usage("You have several vaults; which one?").hint(format!(
                "Pass --vault <name>, or choose one with `{}`.",
                ctx.ome("vault use <name>")
            )),
        ),
    }
}

// Addresses.

/// What names resolve against.
#[derive(Default)]
pub(crate) struct Book {
    pub(crate) contacts: Vec<Contact>,
    pub(crate) aliases: Vec<AztecAlias>,
    pub(crate) vaults: Vec<VaultSummary>,
}

impl Book {
    /// The address book, `aztec-wallet`'s aliases and the vaults; a missing alias store reads as empty.
    pub(crate) async fn load(ctx: &Ctx) -> Result<Self> {
        let client = ctx.client().await?;
        let (contacts, aliases, vaults) = tokio::join!(
            client.call::<ContactsList>(&Empty {}),
            client.call::<AztecAliases>(&Empty {}),
            client.call::<VaultList>(&Empty {}),
        );
        Ok(Self {
            contacts: contacts?,
            aliases: aliases.unwrap_or_default(),
            vaults: vaults?,
        })
    }

    /// A label for an address, from the book.
    pub(crate) fn label(&self, address: &Hex32) -> Option<String> {
        self.contacts
            .iter()
            .find(|c| c.address == *address)
            .map(|c| c.label.clone())
            .or_else(|| {
                self.vaults
                    .iter()
                    .find(|v| v.address == *address)
                    .map(|v| v.name.clone())
            })
            .or_else(|| {
                self.aliases
                    .iter()
                    .find(|a| a.value.parse::<Hex32>().ok() == Some(*address))
                    .map(|a| a.key.clone())
            })
    }

    /// Every address `input` could mean.
    pub(crate) fn matches(&self, input: &str) -> Result<Vec<Candidate<Hex32>>> {
        let input = input.trim();
        if input.starts_with("0x") {
            let address = input.parse::<Hex32>().map_err(|_| {
                Error::usage(format!(
                    "{input:?} is not an address: 0x and 64 hex digits."
                ))
            })?;
            return Ok(vec![Candidate {
                value: address,
                label: self
                    .label(&address)
                    .unwrap_or_else(|| format::short(&address)),
                form: address.to_string(),
            }]);
        }
        let mut out: Vec<Candidate<Hex32>> = Vec::new();
        let mut add = |value: Hex32, label: String, form: String| {
            if !out.iter().any(|c| c.value == value) {
                out.push(Candidate { value, label, form });
            }
        };
        let (kind, name) = input.split_once(':').unwrap_or(("", input));
        if matches!(kind, "accounts" | "contracts") {
            let alias = self
                .aliases
                .iter()
                .find(|a| a.key == input)
                .ok_or_else(|| {
                    Error::usage(format!("aztec-wallet has no alias {input}."))
                        .hint(format!("List them with `aztec-wallet get-alias {kind}`."))
                })?;
            let address = alias.value.parse::<Hex32>().map_err(|_| {
                Error::usage(format!("{input} is {:?}, not an address.", alias.value))
            })?;
            add(address, format!("{input} (aztec-wallet)"), input.to_owned());
            return Ok(out);
        }
        if !kind.is_empty() && kind != "contact" && kind != "vault" {
            return Err(Error::usage(format!("{input:?} is not a reference.")).hint(
                "Use 0x…, accounts:<name>, contracts:<name>, contact:<label> or vault:<name>.",
            ));
        }
        if kind.is_empty() || kind == "contact" {
            for c in &self.contacts {
                if c.label.eq_ignore_ascii_case(name) {
                    add(
                        c.address,
                        format!("{} (contact)", c.label),
                        format!("contact:{}", c.label),
                    );
                }
            }
        }
        if kind.is_empty() || kind == "vault" {
            for v in match_vaults(&self.vaults, name) {
                add(v.address, format!("{} (vault)", v.name), vault_form(v));
            }
        }
        if kind.is_empty() {
            for prefix in ["accounts", "contracts"] {
                let key = format!("{prefix}:{name}");
                if let Some(address) = self
                    .aliases
                    .iter()
                    .find(|a| a.key == key)
                    .and_then(|a| a.value.parse::<Hex32>().ok())
                {
                    add(address, format!("{key} (aztec-wallet)"), key);
                }
            }
        }
        Ok(out)
    }

    /// The one address `input` means.
    pub(crate) fn address(&self, ctx: &Ctx, input: &str) -> Result<Hex32> {
        let candidates = self.matches(input)?;
        if candidates.is_empty() {
            return Err(Error::usage(format!("Nothing is called {input:?}.")).hint(format!(
                "Use 0x…, accounts:<name> or contracts:<name> (aztec-wallet), or add a contact: `{}`.",
                ctx.ome("contact add <address> <label>")
            )));
        }
        pick(ctx, input, candidates)
    }

    /// Resolves the arguments of a call: references become addresses, everything else stays as typed.
    pub(crate) fn call_args(&self, ctx: &Ctx, args: &[String]) -> Result<Vec<String>> {
        args.iter()
            .map(|arg| {
                let reference = ["accounts:", "contracts:", "contact:", "vault:"]
                    .iter()
                    .any(|p| arg.starts_with(p));
                if reference {
                    self.address(ctx, arg).map(|a| a.to_string())
                } else {
                    Ok(arg.clone())
                }
            })
            .collect()
    }
}

// Tokens.

/// The tracked token `input` names: its symbol, name or address.
pub(crate) fn match_tokens<'a>(
    tokens: &'a [TokenInfo],
    input: &str,
    address: Option<Hex32>,
) -> Vec<&'a TokenInfo> {
    tokens
        .iter()
        .filter(|t| {
            Some(t.address) == address
                || t.symbol.eq_ignore_ascii_case(input)
                || t.name.eq_ignore_ascii_case(input)
        })
        .collect()
}

/// A tracked token of the vault.
pub(crate) async fn token(ctx: &Ctx, vault: Hex32, input: &str) -> Result<TokenInfo> {
    let client = ctx.client().await?;
    let tokens: Vec<TokenInfo> = client
        .call::<TokensList>(&VaultParams { vault })
        .await?
        .into_iter()
        .map(|b| b.token)
        .collect();
    let address = if input.starts_with("0x") || input.contains(':') {
        Some(Book::load(ctx).await?.address(ctx, input)?)
    } else {
        None
    };
    let matches = match_tokens(&tokens, input, address);
    let candidates = matches
        .iter()
        .map(|t| Candidate {
            value: t.address,
            label: format!("{} ({})", t.symbol, t.name),
            form: t.address.to_string(),
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(
            Error::usage(format!("{input} is not a token this vault tracks.")).hint(format!(
                "Track it with `{}`.",
                ctx.ome("token add <address>")
            )),
        );
    }
    let address = pick(ctx, input, candidates)?;
    tokens
        .iter()
        .find(|t| t.address == address)
        .cloned()
        .ok_or_else(|| Error::failed("the token vanished"))
}

// Proposals.

/// The proposals whose id starts with `input` (with or without `0x`).
pub(crate) fn match_proposals<'a>(
    proposals: &'a [ProposalView],
    input: &str,
) -> Vec<&'a ProposalView> {
    let digits = input
        .trim()
        .strip_prefix("0x")
        .unwrap_or(input.trim())
        .to_lowercase();
    if digits.is_empty() {
        return Vec::new();
    }
    proposals
        .iter()
        .filter(|p| p.id.to_string()[2..].starts_with(&digits))
        .collect()
}

/// A proposal of the vault: `input`, or in a terminal one picked from those `wanted` keeps.
pub(crate) async fn proposal(
    ctx: &Ctx,
    vault: Hex32,
    input: Option<&str>,
    wanted: impl Fn(&ProposalView) -> bool,
) -> Result<ProposalView> {
    let proposals = ctx
        .client()
        .await?
        .call::<ProposalsList>(&VaultParams { vault })
        .await?;
    let Some(input) = input else {
        let open: Vec<_> = proposals.iter().filter(|p| wanted(p)).collect();
        if open.is_empty() {
            return Err(
                Error::usage("There is no proposal to act on.").hint(format!(
                    "See them all with `{}`.",
                    ctx.ome("proposals --status all")
                )),
            );
        }
        let id = prompt::select(
            ctx,
            "<proposal>",
            "Which proposal?",
            open.iter()
                .map(|p| {
                    (
                        p.id,
                        format!("{}  {}", format::id(&p.id), describe::intent(p)),
                        describe::votes(p),
                    )
                })
                .collect(),
        )?;
        return proposals
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| Error::failed("the proposal vanished"));
    };
    let matches = match_proposals(&proposals, input);
    let candidates = matches
        .iter()
        .map(|p| Candidate {
            value: p.id,
            label: format!("{}  {}", format::id(&p.id), describe::intent(p)),
            form: p.id.to_string()[2..14].to_owned(),
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(
            Error::usage(format!("No proposal of this vault starts with {input:?}.")).hint(
                format!("See them with `{}`.", ctx.ome("proposals --status all")),
            ),
        );
    }
    let id = pick(ctx, input, candidates)?;
    proposals
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::failed("the proposal vanished"))
}

// Members and devices.

/// The committee member `input` names: their index or name.
pub(crate) fn match_members<'a>(
    members: &'a [CommitteeMember],
    input: &str,
) -> Vec<&'a CommitteeMember> {
    if let Ok(index) = input.trim().parse::<u32>() {
        return members.iter().filter(|m| m.index == index).collect();
    }
    members
        .iter()
        .filter(|m| m.name.eq_ignore_ascii_case(input.trim()))
        .collect()
}

pub(crate) fn member<'a>(
    ctx: &Ctx,
    members: &'a [CommitteeMember],
    input: &str,
) -> Result<&'a CommitteeMember> {
    let matches = match_members(members, input);
    let candidates = matches
        .iter()
        .map(|m| Candidate {
            value: m.index,
            label: m.name.clone(),
            form: m.index.to_string(),
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(Error::usage(format!("No member is {input:?}."))
            .hint(format!("See them with `{}`.", ctx.ome("vault members"))));
    }
    let index = pick(ctx, input, candidates)?;
    members
        .iter()
        .find(|m| m.index == index)
        .ok_or_else(|| Error::failed("the member vanished"))
}

/// A paired device: its name or identity prefix.
pub(crate) async fn device(ctx: &Ctx, input: Option<String>) -> Result<DeviceView> {
    let devices = ctx.client().await?.call::<DevicesList>(&Empty {}).await?;
    if devices.is_empty() {
        return Err(Error::usage("No device is paired.")
            .hint(format!("Pair one with `{}`.", ctx.ome("device pair"))));
    }
    let identity = match input {
        None => prompt::select(
            ctx,
            "<device>",
            "Which device?",
            devices
                .iter()
                .map(|d| (d.identity, d.name.clone(), format::short(&d.identity)))
                .collect(),
        )?,
        Some(input) => {
            let digits = hex_prefix(&input);
            let candidates = devices
                .iter()
                .filter(|d| {
                    d.name.eq_ignore_ascii_case(&input)
                        || digits
                            .as_ref()
                            .is_some_and(|p| d.identity.to_string()[2..].starts_with(p))
                })
                .map(|d| Candidate {
                    value: d.identity,
                    label: d.name.clone(),
                    form: d.identity.to_string()[2..14].to_owned(),
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Err(Error::usage(format!("No paired device is {input:?}."))
                    .hint(format!("See them with `{}`.", ctx.ome("device list"))));
            }
            pick(ctx, &input, candidates)?
        }
    };
    devices
        .into_iter()
        .find(|d| d.identity == identity)
        .ok_or_else(|| Error::failed("the device vanished"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(tail: &str) -> Hex32 {
        format!("0x{tail:0>64}").parse().unwrap()
    }

    fn vault(name: &str, alias: Option<&str>, address: Hex32) -> VaultSummary {
        VaultSummary {
            address,
            name: name.into(),
            alias: alias.map(Into::into),
            is_default: false,
            threshold: 2,
            viewing_threshold: 2,
            members: 3,
            my_index: 1,
            founded_at: 0,
        }
    }

    fn book() -> Book {
        Book {
            contacts: vec![
                Contact {
                    address: hex("b0b"),
                    label: "bob".into(),
                },
                Contact {
                    address: hex("ca401"),
                    label: "treasury".into(),
                },
            ],
            aliases: vec![
                AztecAlias {
                    key: "accounts:bob".into(),
                    value: hex("b0b").to_string(),
                },
                AztecAlias {
                    key: "accounts:main".into(),
                    value: hex("a11ce").to_string(),
                },
                AztecAlias {
                    key: "contracts:usdc".into(),
                    value: hex("05dc").to_string(),
                },
                AztecAlias {
                    key: "artifacts:token".into(),
                    value: "/tmp/token.json".into(),
                },
            ],
            vaults: vec![vault("Treasury", Some("treasury"), hex("7a11"))],
        }
    }

    #[test]
    fn typed_forms_resolve_to_one_address() {
        let book = book();
        let one = |input: &str| {
            book.matches(input)
                .unwrap()
                .into_iter()
                .map(|c| c.value)
                .collect::<Vec<_>>()
        };
        assert_eq!(one("accounts:main"), [hex("a11ce")]);
        assert_eq!(one("contracts:usdc"), [hex("05dc")]);
        assert_eq!(one("contact:treasury"), [hex("ca401")]);
        assert_eq!(one("vault:treasury"), [hex("7a11")]);
        assert_eq!(one(&hex("123").to_string()), [hex("123")]);
        assert!(book.matches("accounts:nobody").is_err());
        assert!(book.matches("0x12").is_err());
        assert!(book.matches("secrets:x").is_err());
    }

    #[test]
    fn a_bare_name_searches_everything_and_merges_the_same_address() {
        let book = book();
        // `bob` is a contact and an aztec-wallet account with the same address: one candidate.
        assert_eq!(book.matches("bob").unwrap().len(), 1);
        // `treasury` is a contact and a vault with different addresses: ambiguous, each with its typed form.
        let forms: Vec<_> = book
            .matches("treasury")
            .unwrap()
            .into_iter()
            .map(|c| c.form)
            .collect();
        assert_eq!(forms, ["contact:treasury", "vault:treasury"]);
        assert_eq!(book.matches("main").unwrap()[0].form, "accounts:main");
        assert_eq!(book.matches("nobody").unwrap().len(), 0);
    }

    #[test]
    fn vaults_by_alias_name_address_or_prefix() {
        let vaults = [
            vault("Treasury", Some("t"), hex("7a11")),
            vault("Ops", None, hex("0b5")),
        ];
        assert_eq!(match_vaults(&vaults, "t").len(), 1);
        assert_eq!(match_vaults(&vaults, "ops")[0].name, "Ops");
        assert_eq!(match_vaults(&vaults, "vault:TREASURY")[0].name, "Treasury");
        assert_eq!(match_vaults(&vaults, "0000").len(), 2);
        assert_eq!(match_vaults(&vaults, "abc").len(), 0);
    }

    #[test]
    fn tokens_by_symbol_name_or_address() {
        let usdc = TokenInfo {
            address: hex("05dc"),
            name: "USD Coin".into(),
            symbol: "USDC".into(),
            decimals: 6,
        };
        let tokens = [usdc.clone()];
        assert_eq!(match_tokens(&tokens, "usdc", None), [&usdc]);
        assert_eq!(match_tokens(&tokens, "usd coin", None), [&usdc]);
        assert_eq!(
            match_tokens(&tokens, "contracts:usdc", Some(hex("05dc"))),
            [&usdc]
        );
        assert_eq!(match_tokens(&tokens, "DAI", None).len(), 0);
    }

    #[test]
    fn members_by_index_or_name() {
        let member = |index: u32, name: &str| CommitteeMember {
            index,
            name: name.into(),
            signer_key: ome_api::Point {
                x: hex("1"),
                y: hex("2"),
            },
            verification_share: ome_api::Point {
                x: hex("1"),
                y: hex("2"),
            },
            identity: hex("3"),
            transport: hex("4"),
            peer_id: String::new(),
            addresses: vec![],
            you: false,
        };
        let members = [member(1, "Alice"), member(2, "Bob")];
        assert_eq!(match_members(&members, "2")[0].name, "Bob");
        assert_eq!(match_members(&members, "alice")[0].index, 1);
        assert_eq!(match_members(&members, "carol").len(), 0);
    }
}
