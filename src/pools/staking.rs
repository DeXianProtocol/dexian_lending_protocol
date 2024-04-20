use scrypto::prelude::*;
use crate::utils::*;
use crate::validator::keeper::UnstakeData;

#[blueprint]
#[events(JoinEvent)]
mod staking_pool {

    enable_method_auth!{
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [];
        },
        methods {
            contribute => restrict_to:[operator, OWNER];
            redeem => restrict_to:[operator, OWNER];

            get_redemption_value => PUBLIC;
            get_vault_amount => PUBLIC;
            get_underlying_token => PUBLIC;
        }
    }

    struct StakingResourePool{
        underlying_token: ResourceAddress,
        staking_unit_res_mgr: ResourceManager,
        lsu_map: HashMap<ComponentAddress, Vault>
    }

    impl StakingResourePool {
        
        pub fn instantiate(
            underlying_token: ResourceAddress,
            admin_rule: AccessRule,
            op_rule: AccessRule
        ) -> (Global<StakingResourePool>, ResourceAddress) {
            let (address_reservation, address) =
                Runtime::allocate_component_address(StakingResourePool::blueprint_id());

            let staking_unit_res_mgr: ResourceManager = ResourceBuilder::new_fungible(OwnerRole::Fixed(admin_rule.clone()))
                .metadata(metadata!(init{
                    "pool" => address, locked;
                    "symbol" => "dseXRD", locked;
                    "name" => "DeXian Staking Earning Token ", locked;
                    "icon_url" => "https://dexian.io/images/dse.png", updatable;
                    "info_url" => "https://dexian.io", updatable;

                }))
                .mint_roles(mint_roles! {
                    minter => rule!(require(global_caller(address)));
                    minter_updater => rule!(deny_all);
                })
                .burn_roles(burn_roles! {
                    burner => rule!(require(global_caller(address)));
                    burner_updater => rule!(deny_all);
                })
                .create_with_no_initial_supply();

            let staking_unit_token = staking_unit_res_mgr.address();
            let component = Self {
                lsu_map: HashMap::new(),
                underlying_token,
                staking_unit_res_mgr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .with_address(address_reservation)
            // .metadata(metadata! {
            //     // "pool_resources" => vec![underlying_token, staking_unit_token], locked;
            //     "pool_unit" => staking_unit_token, locked;
            //     }
            // )
            .roles(roles!{
                admin => admin_rule.clone();
                operator => op_rule.clone();
            })
            .globalize();
            
            (component, staking_unit_token)
        }

        pub fn contribute(&mut self, bucket: Bucket, validator_addr: ComponentAddress) -> Bucket{
            assert_resource(&bucket.resource_address(), &self.underlying_token);
            let (_, _, value_per_unit) = self.get_values();
            let mut validator: Global<Validator> = Global::from(validator_addr);
            let amount = bucket.amount();
            let lsu = validator.stake(bucket);

            let lsu_amount = lsu.amount();
            let join_amount = validator.get_redemption_value(lsu_amount);
            let unit_amount = floor_by_resource(self.staking_unit_res_mgr.address(), join_amount.checked_div(value_per_unit).unwrap());
            let unit_bucket = self.staking_unit_res_mgr.mint(unit_amount);

            let lsu_index = amount / lsu_amount;
            let _last_lsu = if self.lsu_map.get(&validator_addr).is_some(){
                let v = self.lsu_map.get_mut(&validator_addr).unwrap();
                v.put(lsu);
                v.amount()
            }
            else{
                self.lsu_map.insert(validator_addr.clone(), Vault::with_bucket(lsu));
                lsu_amount
            };
            Runtime::emit_event(JoinEvent{
                amount: join_amount,
                validator: validator_addr,
                dse_index: value_per_unit,
                dse_amount: unit_bucket.amount(),
                lsu_index,
                lsu_amount
            });

            unit_bucket
        }

        pub fn redeem(&mut self, validator_addr: ComponentAddress, bucket: Bucket) -> (Bucket, NonFungibleLocalId, Decimal){
            assert_resource(&bucket.resource_address(), &self.staking_unit_res_mgr.address());
            assert!(self.lsu_map.get(&validator_addr).is_some(), "the validator address not exists");
            let (_, _, value_per_share) = self.get_values();
            let amount = bucket.amount();
            let redeem_value = amount.checked_mul(value_per_share).unwrap();
            
            let lsu = self.lsu_map.get_mut(&validator_addr).unwrap();
            let mut validator: Global<Validator> = Global::from(validator_addr);
            let lsu_amount = lsu.amount();
            let lsu_value = validator.get_redemption_value(lsu_amount);
            
            
            // assert_amount(lsu_value, redeem_value);
            assert!(redeem_value <= lsu_value, "the target value {} less than expect {}!", lsu_value, redeem_value);
            let lsu_index = lsu_value.checked_div(lsu_amount).unwrap();
            let unstake_lsu_bucket = lsu.take_advanced(redeem_value.checked_div(lsu_index).unwrap(), WithdrawStrategy::Rounded(RoundingMode::ToZero));
            // let unstake_amount = unstake_lsu_bucket.amount();
            let claim_nft = validator.unstake(unstake_lsu_bucket);
            let claim_nft_id = claim_nft.as_non_fungible().non_fungible_local_id();
            let unstake_data = ResourceManager::from_address(claim_nft.resource_address()).get_non_fungible_data::<UnstakeData>(&claim_nft_id);

            self.staking_unit_res_mgr.burn(bucket);
            // Runtime::emit_event(Event2{
            //     amount,
            //     value_per_share,
            //     redeem_value,
            //     lsu_amount,
            //     lsu_value,
            //     unstake_amount,
            //     claim_amount: unstake_data.claim_amount
            // });

            (claim_nft, claim_nft_id, unstake_data.claim_amount)
            
        }

        pub fn get_redemption_value(&self, amount_of_pool_units: Decimal) -> Decimal{
            let(_, _, value_per_unit) = self.get_values();
            amount_of_pool_units.checked_mul(value_per_unit).unwrap()
        }

        pub fn get_vault_amount(&self) -> Decimal{
            self.sum_current_staked()
        }

        pub fn get_underlying_token(&self) -> ResourceAddress{
            self.underlying_token
        }

        fn get_values(&self) -> (Decimal, Decimal, Decimal){
            let total_value = self.get_vault_amount();
            let staking_unit_qty = self.staking_unit_res_mgr.total_supply().unwrap();
            (
                total_value,
                staking_unit_qty,
                if staking_unit_qty.is_zero() {
                    Decimal::ONE
                } else{
                    total_value.checked_div(staking_unit_qty).unwrap()
                }  //value_per_unit
            )
        }

        fn sum_current_staked(& self) -> Decimal {
            let mut sum = Decimal::ZERO;
            for (validator_addr, vault) in &self.lsu_map{
                let validator: Global<Validator> = Global::from(validator_addr.clone());
                let latest = validator.get_redemption_value(vault.amount());
                sum = sum.checked_add(latest).unwrap();
                // Runtime::emit_event(Event1{
                //     sum,
                //     validator: *validator_addr,
                //     lsu_amount: vault.amount(),
                //     staked: latest
                // })
            }
            sum
        }
    }
}


#[derive(ScryptoSbor, ScryptoEvent)]
pub struct JoinEvent {
    pub amount: Decimal,
    pub validator: ComponentAddress,
    pub lsu_index: Decimal,
    pub lsu_amount: Decimal,
    pub dse_index: Decimal,
    pub dse_amount: Decimal,
}

// #[derive(ScryptoSbor, ScryptoEvent)]
// pub struct Event1{
//     pub validator: ComponentAddress,
//     pub lsu_amount: Decimal,
//     pub staked: Decimal,
//     pub sum: Decimal
// }


// #[derive(ScryptoSbor, ScryptoEvent)]
// pub struct Event2{
//     pub amount: Decimal,
//     pub value_per_share: Decimal,
//     pub redeem_value: Decimal,
//     pub lsu_amount: Decimal,
//     pub lsu_value: Decimal,
//     pub unstake_amount: Decimal,
//     pub claim_amount: Decimal
// }
