//! The `devices.*` methods.

use ome_api::{Point, keeper as api};
use ome_protocol::vault::Config;
use ome_wire::{
    messages::device::{DeviceChain, DeviceLabel, DeviceRotation},
    types::WirePoint,
};

use crate::{DeviceReviewRequest, Keeper, Result, store::DeviceRecord};

pub(super) async fn offer(keeper: &Keeper, p: api::DeviceOfferParams) -> Result<api::DeviceOffer> {
    let fee_payers = p.fee_payers.iter().map(|address| address.0).collect();
    let (offer, expires_at) = keeper.offer_device(p.member, p.network, fee_payers).await?;
    Ok(api::DeviceOffer { offer, expires_at })
}

pub(super) async fn list(keeper: &Keeper) -> Result<Vec<api::Device>> {
    Ok(keeper.devices().await.iter().map(device).collect())
}

pub(super) async fn review(
    keeper: &Keeper,
    p: api::DeviceReviewParams,
) -> Result<api::DeviceReviewSent> {
    let points = |points: Vec<Point>| -> Result<Vec<WirePoint>> {
        Ok(points
            .into_iter()
            .map(WirePoint::try_from)
            .collect::<Result<_, _>>()?)
    };
    let rotation = p
        .rotation
        .map(|r| -> Result<DeviceRotation> {
            Ok(DeviceRotation {
                signers: points(r.signers)?,
                threshold: r.threshold,
                viewing_threshold: r.viewing_threshold,
                verification_shares: points(r.verification_shares)?,
            })
        })
        .transpose()?;
    let devices = keeper
        .review_on_devices(DeviceReviewRequest {
            vault: p.vault.0,
            proposal: p.proposal.0,
            vault_name: p.vault_name,
            config: Config::try_from(p.config)?,
            chain: DeviceChain {
                chain_id: p.chain.chain_id.0,
                version: p.chain.version.0,
            },
            labels: p
                .labels
                .into_iter()
                .map(|l| DeviceLabel {
                    address: l.address.0,
                    label: l.label,
                })
                .collect(),
            simulation: p.simulation,
            rotation,
        })
        .await?;
    Ok(api::DeviceReviewSent { devices })
}

fn device(record: &DeviceRecord) -> api::Device {
    api::Device {
        identity: record.identity.0.into(),
        transport: record.transport.into(),
        signer_key: Point::from(&record.signer_key),
        name: record.name.clone(),
        paired_at: record.paired_at,
    }
}
