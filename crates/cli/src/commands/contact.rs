//! `ome contact add|list|remove`: labels for addresses, local to this profile. Reviews say when a label came from
//! here, since nobody verified it.

use ome_api::{
    Empty,
    daemon::{AddressParams, ContactParams, ContactsList, ContactsPut, ContactsRemove},
};

use crate::{
    args::ContactCommand,
    context::{Ctx, Mode},
    error::{Error, Result},
    refs::Book,
    ui::{format, prompt, table, theme},
};

fn valid_label(label: &str) -> std::result::Result<(), String> {
    let ok = !label.is_empty()
        && label.len() <= 32
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err("Lowercase letters, digits, - and _, at most 32.".into())
    }
}

pub(crate) async fn run(ctx: &Ctx, command: ContactCommand) -> Result<()> {
    let client = ctx.client().await?;
    let contacts = match command {
        ContactCommand::Add { address, label } => {
            let input = prompt::text(
                ctx,
                address,
                "<address>",
                "The address (0x… or accounts:<name>)",
                None,
                |_| Ok(()),
            )?;
            let address = Book::load(ctx).await?.address(ctx, &input)?;
            let label = prompt::text(ctx, label, "<label>", "Its label", None, valid_label)?;
            let contacts = client
                .call::<ContactsPut>(&ContactParams {
                    address,
                    label: label.clone(),
                })
                .await?;
            ctx.success(&format!(
                "{} is {}",
                format::short(&address),
                theme::name(format!("contact:{label}"))
            ));
            contacts
        }
        ContactCommand::List => client.call::<ContactsList>(&Empty {}).await?,
        ContactCommand::Remove { contact } => {
            let contacts = client.call::<ContactsList>(&Empty {}).await?;
            let input = match contact {
                Some(input) => input,
                None => prompt::select(
                    ctx,
                    "<contact>",
                    "Which contact?",
                    contacts
                        .iter()
                        .map(|c| (c.label.clone(), c.label.clone(), format::short(&c.address)))
                        .collect(),
                )?,
            };
            let input = input.strip_prefix("contact:").unwrap_or(&input);
            let found = contacts
                .iter()
                .find(|c| c.label == input || c.address.to_string() == input)
                .ok_or_else(|| Error::usage(format!("No contact is {input:?}.")))?;
            let contacts = client
                .call::<ContactsRemove>(&AddressParams {
                    address: found.address,
                })
                .await?;
            ctx.success(&format!("Removed {}", found.label));
            contacts
        }
    };
    if ctx.mode == Mode::Json {
        return ctx.json(&contacts);
    }
    let mut t = table::Table::new(&["label", "address"]);
    for c in &contacts {
        t.row(vec![
            table::toned(&c.label, table::Tone::Name),
            table::cell(c.address.to_string()),
        ]);
    }
    if !t.is_empty() {
        t.print(ctx);
    }
    Ok(())
}
