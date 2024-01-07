use scrypto::prelude::*;
use crate::utils::*;
use crate::pools::staking::staking_pool::*;
use crate::cdp::cdp_mgr::CollateralDebtManager;
use crate::validator::keeper::UnstakeData;
use crate::validator::keeper::validator_keeper::ValidatorKeeper;



#[blueprint]
#[events(JoinEvent, FasterRedeemEvent, NormalRedeemEvent, NftFasterRedeemEvent, ClaimXrdEvent)]
mod staking_earning {

    enable_method_auth! {
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [admin];
        },
        methods {
            set_unstake_epoch_num => restrict_to: [operator, OWNER];
            join => restrict_to: [operator, OWNER];
            claim_xrd => restrict_to: [operator, OWNER];
            redeem => restrict_to: [operator, OWNER];
            
            get_dse_token => PUBLIC;
        }
    }

    struct StakingEarning{
        validator_keeper: Global<ValidatorKeeper>,
        staking_pool: Global<StakingResourePool>,
        claim_nft_map: KeyValueStore<ResourceAddress, NonFungibleVault>,
        dse_token: ResourceAddress,
        // dx_token: ResourceAddress,
        unstake_epoch_num: u64

    }

    impl StakingEarning{

        pub fn instantiate(
            validator_keeper: Global<ValidatorKeeper>,
            unstake_epoch_num: u64,
            admin_rule: AccessRule,
            op_rule: AccessRule
        ) -> Global<StakingEarning>{
            let (address_reservation, component_address) = Runtime::allocate_component_address(
                StakingEarning::blueprint_id()
            );
            let caller_rule = rule!(require(global_caller(component_address)));
            let (staking_pool,dse_token) = StakingResourePool::instantiate(XRD, admin_rule.clone(), caller_rule);
            

            let component = Self{
                claim_nft_map: KeyValueStore::new(),
                validator_keeper,
                staking_pool,
                dse_token,
                // dx_token,
                unstake_epoch_num
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .with_address(address_reservation)
            .roles(roles! {
                admin => admin_rule.clone();
                operator => op_rule.clone();
            })
            .globalize();
            component
        }

        ///
        /// claim xrd with claimNFT
        /// 
        pub fn claim_xrd(&mut self, cdp_mgr: Global<CollateralDebtManager>, validator_addr:ComponentAddress, claim_nft_bucket: Bucket) -> (Bucket, Decimal){
            let nft_addr = claim_nft_bucket.resource_address();            
            let mut validator: Global<Validator> = Global::from(validator_addr);

            let res_mgr = ResourceManager::from(nft_addr.clone());
            let nft_bucket = claim_nft_bucket.as_non_fungible();
            let current_epoch = Runtime::current_epoch().number();
            let nft_id = nft_bucket.non_fungible_local_id();
            let unstake_data = res_mgr.get_non_fungible_data::<UnstakeData>(&nft_id);
            let claim_amount = unstake_data.claim_amount;
            let claim_epoch = unstake_data.claim_epoch.number();
            if claim_epoch <= current_epoch {
                let bucket = validator.claim_xrd(claim_nft_bucket);
                Runtime::emit_event(ClaimXrdEvent{
                    rate: Decimal::ZERO,
                    receive: bucket.amount(),
                    nft_addr,
                    nft_id,
                    claim_amount,
                    claim_epoch,
                    current_epoch
                });
                (bucket, claim_amount)
            }
            else{
                let (_, stable_rate, _) = cdp_mgr.get_interest_rate(XRD, unstake_data.claim_amount);
                let remain_epoch = claim_epoch - current_epoch;
                let principal = calc_principal(unstake_data.claim_amount, stable_rate, Decimal::from(EPOCH_OF_YEAR), remain_epoch);
                let bucket = cdp_mgr.staking_borrow(XRD, principal, stable_rate);
                
                if self.claim_nft_map.get(&nft_addr).is_some() {
                    self.claim_nft_map.get_mut(&nft_addr).unwrap().put(nft_bucket);
                }
                else{
                    self.claim_nft_map.insert(nft_addr.clone(), NonFungibleVault::with_bucket(nft_bucket));
                }   
                Runtime::emit_event(ClaimXrdEvent{
                    rate: stable_rate,
                    receive: bucket.amount(),
                    nft_addr,
                    nft_id,
                    claim_amount,
                    claim_epoch,
                    current_epoch
                });
                (bucket, claim_amount)
            }
        }

        pub fn join(&mut self, validator_addr: ComponentAddress, bucket: Bucket) -> Bucket{
            // assert!(self.staking_pool.get_underlying_token() == bucket.resource_address(), "the unsupported token!");
            let amount = bucket.amount();
            let unit_bucket = self.staking_pool.contribute(bucket, validator_addr);
            Runtime::emit_event(JoinEvent{
                validator:validator_addr,
                unit_amount:unit_bucket.amount(),
                amount,
            });
            unit_bucket
        }

        pub fn redeem(&mut self, cdp_mgr: Global<CollateralDebtManager>, validator_addr: ComponentAddress,  bucket: Bucket, faster: bool) -> Bucket{
            let res_addr = bucket.resource_address();
            let amount = bucket.amount();
            let (claim_nft_bucket, claim_nft_id, claim_amount) = if res_addr == self.dse_token {
                 self.staking_pool.redeem(validator_addr, bucket)
            }
            else{
                let mut validator: Global<Validator> = Global::from(validator_addr);
                let claim_nft = validator.unstake(bucket);
                let claim_nft_id = claim_nft.as_non_fungible().non_fungible_local_id();
                let unstake_data = ResourceManager::from_address(claim_nft.resource_address()).get_non_fungible_data::<UnstakeData>(&claim_nft_id);
                (claim_nft, claim_nft_id, unstake_data.claim_amount)
            };
            
            if faster {
                let (xrd_bucket, _) = self.claim_xrd(cdp_mgr, validator_addr, claim_nft_bucket);
                let xrd_amount = xrd_bucket.amount();
                Runtime::emit_event(FasterRedeemEvent{
                    fee: claim_amount.checked_sub(xrd_amount).unwrap(),
                    res_addr,
                    amount,
                    validator_addr,
                    xrd_amount
                });
                xrd_bucket
            }
            else{
                Runtime::emit_event(NormalRedeemEvent{
                    claim_nft: claim_nft_bucket.resource_address(),
                    res_addr,
                    amount,
                    validator_addr,
                    claim_nft_id,
                    claim_amount
                });
                claim_nft_bucket
            }
        }

        pub fn set_unstake_epoch_num(&mut self, unstake_epoch_num: u64){
            self.unstake_epoch_num = unstake_epoch_num;
        }

        pub fn get_dse_token(&self) -> ResourceAddress{
            self.dse_token
        }

    }
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct JoinEvent {
    pub amount: Decimal,
    pub validator: ComponentAddress,
    pub unit_amount: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct FasterRedeemEvent{
/// resource address of LSUs or DSE
    pub res_addr: ResourceAddress,
    pub amount: Decimal,
    pub validator_addr: ComponentAddress,
    pub xrd_amount: Decimal,
    pub fee: Decimal
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct NormalRedeemEvent{
/// resource address of LSUs or DSE
    pub res_addr: ResourceAddress,
    pub amount: Decimal,
    pub validator_addr: ComponentAddress,
    pub claim_nft: ResourceAddress,
    pub claim_nft_id: NonFungibleLocalId,
    pub claim_amount: Decimal
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct NftFasterRedeemEvent{
    pub res_addr: ResourceAddress,
    pub nft_id: NonFungibleLocalId,
    pub claim_amount: Decimal,
    pub claim_epoch: Decimal,
    pub validator_addr: ComponentAddress,
    pub xrd_amount: Decimal,
    pub fee: Decimal
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ClaimXrdEvent{
    pub rate: Decimal,
    pub receive: Decimal,
    pub nft_addr: ResourceAddress,
    pub nft_id: NonFungibleLocalId,
    pub claim_amount: Decimal,
    pub claim_epoch: u64,
    pub current_epoch: u64
}