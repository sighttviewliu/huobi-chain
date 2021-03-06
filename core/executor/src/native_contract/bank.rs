use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use bytes::Bytes;
use derive_more::{Display, From};

use protocol::traits::executor::contract::{BankContract, ContractStateAdapter};
use protocol::traits::executor::RcInvokeContext;
use protocol::types::{Asset, AssetID, Balance, ContractAddress, ContractType, Hash};
use protocol::{ProtocolError, ProtocolErrorKind, ProtocolResult};

use crate::cycles::{consume_cycles, CyclesAction};
use crate::fixed_types::FixedAssetSchema;

/// Bank is the registration and query center for asset.
///
/// It only does two things
/// 1. Responsible for generating a unique ID for the asset and writing the
/// asset's information to the chain.
/// 2. Query the basic information of the asset by asset id.
pub struct NativeBankContract<StateAdapter: ContractStateAdapter> {
    chain_id: Hash,

    state_adapter: Rc<RefCell<StateAdapter>>,
}

impl<StateAdapter: ContractStateAdapter> NativeBankContract<StateAdapter> {
    pub fn new(chain_id: Hash, state_adapter: Rc<RefCell<StateAdapter>>) -> Self {
        Self {
            chain_id,
            state_adapter,
        }
    }
}

impl<StateAdapter: ContractStateAdapter> BankContract<StateAdapter>
    for NativeBankContract<StateAdapter>
{
    // Register an asset.
    // The asset id is generated by: AssetID = Hash(ChainID + AssetContractAddress).
    //
    // NOTE: After the asset is successfully registered, the `world state` will not
    // be modified unless `commit` is called.
    fn register(
        &mut self,
        ictx: RcInvokeContext,
        address: &ContractAddress,
        name: String,
        symbol: String,
        supply: Balance,
    ) -> ProtocolResult<Asset> {
        if address.contract_type() != ContractType::Asset {
            return Err(NativeBankContractError::InvalidAddress.into());
        }

        let asset_id = Hash::digest(Bytes::from(
            [self.chain_id.as_bytes(), address.as_bytes()].concat(),
        ));

        // Although the probability of a collision is small, we should still check it.
        if self
            .state_adapter
            .borrow()
            .contains::<FixedAssetSchema>(&asset_id)?
        {
            return Err(NativeBankContractError::AssetExists { id: asset_id }.into());
        }

        let asset = Asset {
            name,
            symbol,
            supply,

            id: asset_id.clone(),
            manage_contract: address.clone(),
            storage_root: Hash::from_empty(),
        };

        self.state_adapter
            .borrow_mut()
            .insert_cache::<FixedAssetSchema>(asset_id, asset.clone())?;

        let cycles_used = consume_cycles(
            CyclesAction::BankRegister,
            ictx.borrow().cycles_used,
            ictx.borrow().cycles_limit,
        )?;
        ictx.borrow_mut().cycles_used = cycles_used;
        Ok(asset)
    }

    fn get_asset(&self, _ictx: RcInvokeContext, id: &AssetID) -> ProtocolResult<Asset> {
        let fixed_asset: Asset = self
            .state_adapter
            .borrow()
            .get::<FixedAssetSchema>(&id)?
            .ok_or(NativeBankContractError::NotFound { id: id.clone() })?;
        Ok(fixed_asset)
    }
}

#[derive(Debug, Display, From)]
pub enum NativeBankContractError {
    #[display(fmt = "asset id {:?} already exists", id)]
    AssetExists { id: AssetID },

    #[display(fmt = "asset id {:?} not found", id)]
    NotFound { id: AssetID },

    #[display(fmt = "invalid address")]
    InvalidAddress,
}

impl Error for NativeBankContractError {}

impl From<NativeBankContractError> for ProtocolError {
    fn from(err: NativeBankContractError) -> ProtocolError {
        ProtocolError::new(ProtocolErrorKind::Executor, Box::new(err))
    }
}
