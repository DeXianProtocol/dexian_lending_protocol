use scrypto::prelude::*;
use crate::utils::*;
use crate::pools::staking::staking_pool::*;
use crate::pools::lending::lend_pool::*;
use crate::validator::keeper::UnstakeData;
use crate::validator::keeper::validator_keeper::ValidatorKeeper;



#[blueprint]
#[events(JoinEvent, RedeemEvent)]
mod staking_earning {

    enable_method_auth! {
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [admin];
        },
        methods {
            // new_pool => restrict_to: [admin, OWNER];
            // withdraw_fee => restrict_to: [admin, OWNER];  // withdraw_fee should restrict to Pool?
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
        claim_nft_map: HashMap<ResourceAddress, NonFungibleVault>,
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
                claim_nft_map: HashMap::new(),
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
        pub fn claim_xrd(&mut self, lending_pool: Global<LendResourcePool>, validator_addr:ComponentAddress, claim_nft_bucket: Bucket) -> Bucket{
            let nft_addr = claim_nft_bucket.resource_address();            
            let mut validator: Global<Validator> = Global::from(validator_addr);

            let mut bucket = Bucket::new(XRD);
            let res_mgr = ResourceManager::from(nft_addr);
            let current_epoch = Runtime::current_epoch().number();
            let mut nft_bucket = claim_nft_bucket.as_non_fungible();
            nft_bucket.non_fungible_local_ids().iter().for_each(|nft_local_id|{
                let unstake_data = res_mgr.get_non_fungible_data::<UnstakeData>(nft_local_id);
                if unstake_data.claim_epoch.number() <= current_epoch {
                    bucket.put(validator.claim_xrd(nft_bucket.take_non_fungible(nft_local_id).into()));
                }
                else{
                    let (_, _, stable_rate) = lending_pool.get_interest_rate();
                    let remain_epoch = unstake_data.claim_epoch.number() - current_epoch;
                    let principal = calc_principal(unstake_data.claim_amount, stable_rate, Decimal::from(EPOCH_OF_YEAR), remain_epoch);
                    bucket.put(lending_pool.borrow_stable(principal, stable_rate));
                    let mut vault = Vault::new(nft_addr).as_non_fungible();
                    vault.put(nft_bucket.take_non_fungible(nft_local_id));
                    self.claim_nft_map.entry(nft_addr).and_modify(|v|{
                        v.put(nft_bucket.take_non_fungible(nft_local_id));
                    }).or_insert(vault);
                }
            });

            bucket
        }

        pub fn join(&mut self, validator_addr: ComponentAddress, bucket: Bucket) -> Bucket{
            // assert!(self.staking_pool.get_underlying_token() == bucket.resource_address(), "the unsupported token!");
            self.staking_pool.contribute(bucket, validator_addr)
        }

        pub fn redeem(&mut self, lending_pool: Global<LendResourcePool>, validator_addr: ComponentAddress,  bucket: Bucket, faster: bool) -> Bucket{
            let res_addr = bucket.resource_address();
            let claim_nft_bucket = if res_addr == self.dse_token {
                 self.staking_pool.redeem(validator_addr, bucket)
            }
            else{
                let mut validator: Global<Validator> = Global::from(validator_addr);
                validator.unstake(bucket)
            };
            
            if faster {
                self.claim_xrd(lending_pool, validator_addr, claim_nft_bucket)
            }
            else{
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
    pub token: ResourceAddress,
    pub amout: Decimal,
    pub share_amount: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
// pub struct RedeemEvent([(ResourceAddress, Decimal); 2]);
pub struct RedeemEvent{
    
    /// resource address of LSUs
    pub token: ResourceAddress,
    pub amount: Decimal,
    pub faster: bool,
    pub validator_addr: ComponentAddress,
    pub receive_token: ResourceAddress,
    pub receive_amount: Option<Decimal>,
    pub receive_nft_id: Option<NonFungibleLocalId>
}

// #[derive(ScryptoSbor, ScryptoEvent)]
// pub struct RemoveLiquidityEvent {
//     pub pool_units_amount: Decimal,
//     pub redeemed_resources: [(ResourceAddress, Decimal); 2],
// }

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ClaimEvent {
    pub res_addr: (ResourceAddress, Decimal),
    pub caim_amount: Decimal,
    pub claim_epoch_at: u64
}