use scrypto::prelude::*;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
use crate::utils::*;

#[derive(ScryptoSbor, NonFungibleData)]
pub struct FlashLoanData{
    pub res_addr: ResourceAddress,
    pub amount: Decimal,
    pub fee: Decimal
}

#[derive(ScryptoSbor, NonFungibleData)]
pub struct CollateralDebtPosition{
    pub borrow_token: ResourceAddress,
    pub collateral_token: ResourceAddress,

    #[mutable]
    pub is_stable: bool,
    //The total amount borrowed from the user's perspective.
    #[mutable]
    pub total_borrow: Decimal,
    //The total amount repaid from the user's perspective.
    #[mutable]
    pub total_repay: Decimal,
    
    #[mutable]
    pub normalized_borrow: Decimal,
    #[mutable]
    pub collateral_amount: Decimal,
    #[mutable]
    pub borrow_amount: Decimal,
    
    // for stable
    #[mutable]
    pub last_update_epoch: u64,
    #[mutable]
    pub stable_rate: Decimal
}

#[derive(ScryptoSbor)]
struct AssetState{
    // pub def_interest_model: ComponentAddress,
    pub interest_model: InterestModel,
    pub collateral_token: ResourceAddress,
    pub ltv: Decimal,
    pub liquidation_threshold: Decimal,
    pub liquidation_bonus: Decimal
}


#[blueprint]
mod cdp_mgr{
    
    enable_method_auth!{
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [];
            protocol_caller => updatable_by:[];
        },
        methods{
            new_pool => restrict_to:[operator, OWNER];
            withdraw_insurance => restrict_to: [operator, OWNER];
            set_close_factor =>restrict_to: [operator, OWNER];

            borrow_variable => restrict_to: [protocol_caller, OWNER];
            borrow_stable => restrict_to: [protocol_caller, OWNER];
            extend_borrow => restrict_to: [protocol_caller, OWNER];
            withdraw_collateral => restrict_to:[protocol_caller, OWNER];
            liquidation => restrict_to:[protocol_caller, OWNER];

            staking_borrow => restrict_to: [protocol_caller, OWNER];
            staking_repay => restrict_to: [protocol_caller, OWNER];

            borrow_flashloan => PUBLIC;
            repay_flashloan => PUBLIC;
            supply => PUBLIC;
            withdraw => PUBLIC;
            repay => PUBLIC;
            addition_collateral => PUBLIC;
            get_underlying_token => PUBLIC;
            get_cdp_resource_address => PUBLIC;
            get_interest_rate => PUBLIC;
        }
    }

    struct CollateralDebtManager{
        price_oracle: Global<PriceOracle>,
        // Lend Pool of each asset in the lending pool. I.E.: XRD ==> LendResourcePool(XRD)
        pools: HashMap<ResourceAddress, Global<LendResourcePool>>,
        //Status of each asset in the lending pool, I.E.: XRD ==> AssetState(XRD)
        states: HashMap<ResourceAddress, AssetState>,
        // address map for supply token(K) and deposit token(V), I.E. dxXRD --> XRD
        deposit_asset_map: KeyValueStore<ResourceAddress, ResourceAddress>,
        // vault for each collateral asset(supply token), I.E. dxXRD ==> Vault(dxXRD)
        collateral_vaults: KeyValueStore<ResourceAddress, Vault>,
        // CDP token define
        cdp_res_mgr: ResourceManager,
        // CDP id counter
        cdp_id_counter: u64,
        self_cmp_addr: ComponentAddress,
        // close factor for liquidation
        close_factor_percent: Decimal,
        /// flashloan NFT resource manager
        transient_nft_res_mgr: ResourceManager,
        // flashloan NFT counter
        transient_id_counter: u64,
        cdp_vault: NonFungibleVault
    }

    impl CollateralDebtManager{

        /// Collateral Debt Position Manager
        pub fn instantiate(
            admin_rule: AccessRule, 
            pool_mgr_rule: AccessRule,
            caller_rule: AccessRule,
            price_oracle: Global<PriceOracle>
        )->(Global<CollateralDebtManager>, ResourceAddress){
            let (address_reservation, address) = Runtime::allocate_component_address(CollateralDebtManager::blueprint_id());
            let cdp_res_mgr = ResourceBuilder::new_integer_non_fungible::<CollateralDebtPosition>(OwnerRole::None)
                .metadata(metadata!(init{
                    "symbol" => "CDP", locked;
                    "name" => "DeXian CDP Token", locked;
                }))
                .mint_roles(mint_roles!( 
                    minter => rule!(require(global_caller(address)));
                    minter_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(address)));
                    burner_updater => rule!(deny_all);
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(global_caller(address)));
                    non_fungible_data_updater_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply();

            let transient_nft_res_mgr = ResourceBuilder::new_integer_non_fungible::<FlashLoanData>(
                OwnerRole::Fixed(admin_rule.clone())
            ).metadata(metadata!{
                init{
                    "name"=> "dxLoanNFT", locked;
                    "description" => "DeXian FlashLoan NFT", locked;
                    "dapp_definitions" => "DeXian Protocol", locked;
                }
            }).mint_roles(mint_roles!(
                minter => rule!(require(global_caller(address)));
                minter_updater => rule!(deny_all);
            )).burn_roles(burn_roles!(
                burner => rule!(require(global_caller(address)));
                burner_updater => rule!(deny_all);
            )).deposit_roles(deposit_roles!(
                depositor => rule!(deny_all);
                depositor_updater => rule!(deny_all);
            )).create_with_no_initial_supply();
            
            let component = Self{
                pools: HashMap::new(),
                states: HashMap::new(),
                deposit_asset_map: KeyValueStore::new(),
                collateral_vaults: KeyValueStore::new(),
                self_cmp_addr: address,
                close_factor_percent: Decimal::from(50),
                cdp_id_counter: 0u64,
                transient_id_counter: 0u64,
                cdp_vault: NonFungibleVault::new(cdp_res_mgr.address()),
                price_oracle,
                cdp_res_mgr,
                transient_nft_res_mgr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .with_address(address_reservation)
            .roles(roles!{
                admin => admin_rule.clone();
                operator => pool_mgr_rule.clone();
                protocol_caller => caller_rule.clone();
            }
            )
            .globalize();
            (component, cdp_res_mgr.address())
        }

        pub fn new_pool(&mut self, 
            share_divisibility: u8,
            underlying_token_addr: ResourceAddress,
            interest_model: InterestModel,
            interest_model_cmp_addr: ComponentAddress,
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal,
            flashloan_fee_ratio: Decimal,
            admin_rule: AccessRule,
            protocol_caller: Option<ComponentAddress>
        ) -> ResourceAddress{
            let pool_mgr_rule = if protocol_caller.is_some(){
                rule!(
                    require(global_caller(self.self_cmp_addr)) ||
                    require(global_caller(protocol_caller.unwrap()))
                )
            }
            else{
                rule!(require(global_caller(self.self_cmp_addr)))
            };
            let (lend_res_pool, dx_token_addr) = LendResourcePool::instantiate(
                share_divisibility,
                underlying_token_addr, 
                interest_model_cmp_addr,
                interest_model.clone(),
                insurance_ratio,
                flashloan_fee_ratio,
                admin_rule,
                pool_mgr_rule
                );
            let asset_state = AssetState{
                interest_model: interest_model.clone(),
                collateral_token: dx_token_addr,
                ltv,
                liquidation_threshold,
                liquidation_bonus
            };
            self.pools.insert(underlying_token_addr, lend_res_pool);
            self.states.insert(underlying_token_addr, asset_state);
            self.collateral_vaults.insert(dx_token_addr, Vault::new(dx_token_addr));
            self.deposit_asset_map.insert(dx_token_addr, underlying_token_addr);
            dx_token_addr
        }

        pub fn staking_borrow(&mut self, underlying_token_addr: ResourceAddress, borrow_amount: Decimal, stable_rate: Decimal,
            collateral_token: ResourceAddress, collateral_amount: Decimal
        ) -> (Bucket, NonFungibleLocalId){
            assert!(self.pools.get(&underlying_token_addr).is_some(), "There is no pool of funds corresponding to the assets!");
            let borrow_bucket = self.staking_borrow_from_pool(underlying_token_addr, borrow_amount, stable_rate);
            //mint cdp
            let cdp_bucket = self.new_cdp(collateral_token, underlying_token_addr, borrow_amount, collateral_amount, Decimal::ZERO, stable_rate, true).as_non_fungible();
            let cdp_nft_id = cdp_bucket.non_fungible_local_id();
            self.cdp_vault.put(cdp_bucket);
            (borrow_bucket, cdp_nft_id)
        }

        fn staking_borrow_from_pool(&mut self, underlying_token_addr: ResourceAddress, borrow_amount: Decimal, stable_rate: Decimal)->Bucket{
            let lending_pool = self.pools.get_mut(&underlying_token_addr).unwrap();
            lending_pool.borrow_stable(borrow_amount, stable_rate)
        }

        pub fn staking_repay(&mut self, repay_bucket: Bucket, cdp_id: NonFungibleLocalId) -> (Bucket, Decimal, Decimal){
            let underlying_token_addr = repay_bucket.resource_address();
            assert!(self.pools.get(&underlying_token_addr).is_some(), "There is no pool of funds corresponding to the assets!");
            let repay_amount = repay_bucket.amount();
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let (bucket, actual_repay_amount, repay_in_borrow) = self.repay_stable_to_pool(underlying_token_addr, repay_bucket, cdp_data.borrow_amount, cdp_data.stable_rate, cdp_data.last_update_epoch, None);
            if repay_amount >= actual_repay_amount {
                self.cdp_res_mgr.burn(self.cdp_vault.take_non_fungible(&cdp_id));
            }
            else{
                self.update_cdp_after_repay(&cdp_id, cdp_data, actual_repay_amount, repay_in_borrow, Decimal::ZERO, Decimal::ZERO);
            }
            (bucket, actual_repay_amount, repay_in_borrow)
        }

        pub fn get_interest_rate(&self, underlying_token_addr: ResourceAddress, stable_borrow_amount:Decimal) -> (Decimal, Decimal, Decimal){
            assert!(self.pools.get(&underlying_token_addr).is_some(), "There is no pool of funds corresponding to the assets!");
            let lending_pool = self.pools.get(&underlying_token_addr).unwrap();
            lending_pool.get_interest_rate(stable_borrow_amount)
        }

        pub fn set_close_factor(&mut self, new_close_factor: Decimal){
            self.close_factor_percent = new_close_factor;
        }

        pub fn supply(&mut self, bucket: Bucket) -> Bucket{
            let supply_res_addr = bucket.resource_address();
            assert!(self.pools.get(&supply_res_addr).is_some(), "There is no pool of funds corresponding to the assets!");
            let lending_pool = self.pools.get_mut(&supply_res_addr).unwrap();
            lending_pool.add_liquity(bucket)
        }

        pub fn withdraw(&mut self, bucket: Bucket) -> Bucket{
            let dx_token = bucket.resource_address();
            assert!(self.deposit_asset_map.get(&dx_token).is_some(), "the token has not supported!");
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let lending_pool = self.pools.get_mut(&underlying_token).unwrap();
            lending_pool.remove_liquity(bucket)
        }

        pub fn borrow_variable(&mut self,
            dx_bucket: Bucket,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            let dx_token = dx_bucket.resource_address();
            let dx_amount = dx_bucket.amount();
            let max_loan_amount = self.get_max_loan_amount(dx_token, dx_amount, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd, Decimal::ZERO);
            assert!(borrow_amount <= max_loan_amount, "The amount borrowed exceeds the borrowable quantity of the collateral.");

            self.put_collateral_vault(dx_bucket);
            let (borrow_bucket, borrow_normalized_amount) = self.borrow_variable_from_pool(borrow_token, borrow_amount);
            //mint cdp
            let cdp_bucket = self.new_cdp(dx_token, borrow_token, borrow_amount, dx_amount, borrow_normalized_amount, Decimal::ZERO, false);
            (borrow_bucket, cdp_bucket)
        }

        fn borrow_variable_from_pool(&mut self, borrow_token: ResourceAddress, borrow_amount: Decimal) -> (Bucket, Decimal){
            let lending_pool = self.pools.get_mut(&borrow_token).unwrap();
            lending_pool.borrow_variable(borrow_amount)
        }

        pub fn borrow_stable(&mut self,
            dx_bucket: Bucket,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            let dx_token = dx_bucket.resource_address();
            let dx_amount = dx_bucket.amount();
            let max_loan_amount = self.get_max_loan_amount(dx_token, dx_amount, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd, Decimal::ZERO);
            assert!(borrow_amount <= max_loan_amount, "The amount borrowed exceeds the borrowable quantity of the collateral.");
            
            self.put_collateral_vault(dx_bucket);
            let (borrow_bucket, stable_rate) = self.borrow_stable_from_pool(borrow_token, borrow_amount);
            
            //mint cdp
            let cdp_bucket = self.new_cdp(dx_token, borrow_token, borrow_amount, dx_amount, Decimal::ZERO, stable_rate, true);
            (borrow_bucket, cdp_bucket)
        }

        fn borrow_stable_from_pool(&mut self, borrow_token: ResourceAddress, borrow_amount:Decimal) -> (Bucket, Decimal){
            let lending_pool = self.pools.get_mut(&borrow_token).unwrap();
            let (_variable_rate,stable_rate,_supply_rate) = lending_pool.get_interest_rate(borrow_amount);
            (lending_pool.borrow_stable(borrow_amount, stable_rate), stable_rate)
        }

        pub fn extend_borrow(&mut self,
            cdp: Bucket,
            amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            assert_resource(&cdp.resource_address(), &self.cdp_res_mgr.address());
            assert!(cdp.as_non_fungible().amount() == Decimal::ONE, "Only one CDP can be processed at a time!");
            
            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;
            let dx_amount = cdp_data.collateral_amount;
            let max_loan_amount = self.get_max_loan_amount(dx_token, dx_amount, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd, Decimal::ZERO);
            
            let mut cdp_avg_rate = Decimal::ZERO;
            let mut interest = Decimal::ZERO;
            let mut delta_normalized_amount = Decimal::ZERO;
            
            let borrow_bucket: Bucket = if cdp_data.is_stable {
                let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();
                interest = borrow_pool.get_stable_interest(cdp_data.borrow_amount, cdp_data.last_update_epoch, cdp_data.stable_rate);
                let borrow_intent = cdp_data.borrow_amount.checked_add(interest).unwrap().checked_add(amount).unwrap();
                info!("exist stable: {}:{},{},{}", borrow_token.to_hex(), cdp_data.borrow_amount, interest, borrow_intent);
                assert_amount(borrow_intent, max_loan_amount);
                
                let (_variable_rate, stable_rate, _supply_rate)  = borrow_pool.get_interest_rate(amount);
                let borrow_bucket = borrow_pool.borrow_stable(amount, stable_rate);
                cdp_avg_rate = get_weight_rate(cdp_data.borrow_amount.checked_add(interest).unwrap(), cdp_data.stable_rate, amount, stable_rate);

                borrow_bucket
            }
            else{
                let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();
                let (_, current_borrow_index) = borrow_pool.get_current_index();
                let exist_borrow = cdp_data.normalized_borrow.checked_mul(current_borrow_index).unwrap();
                let borrow_intent = exist_borrow.checked_add(amount).unwrap();
                info!("exist variable: {}:{}*{},{}", borrow_token.to_hex(), cdp_data.normalized_borrow,current_borrow_index, borrow_intent);
                assert_amount(borrow_intent, max_loan_amount);
                let (borrow_bucket, normalized_amount) = borrow_pool.borrow_variable(amount);
                delta_normalized_amount = normalized_amount;
                borrow_bucket
            };
            self.update_cdp_data(cdp_data.is_stable, amount, interest, Decimal::ZERO, delta_normalized_amount, cdp_avg_rate, cdp_id, cdp_data);
            
            (borrow_bucket, cdp)
        }

        pub fn withdraw_collateral(&mut self,
            cdp: Bucket,
            amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            assert_resource(&cdp.resource_address(), &self.cdp_res_mgr.address());
            assert!(cdp.as_non_fungible().amount() == Decimal::ONE, "Only one CDP can be processed at a time!");
            
            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;
            let dx_amount = cdp_data.collateral_amount;

            self.validate_withdraw_collateral(dx_token, dx_amount, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd, cdp_data.normalized_borrow, amount);

            let divisibility = get_divisibility(dx_token.clone()).unwrap();
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let underlying_pool = self.pools.get_mut(&underlying_token).unwrap();
            let (supply_index, _) = underlying_pool.get_current_index();
            
            let take_amount = floor(amount.checked_div(supply_index).unwrap(), divisibility);
            let dx_bucket = self.collateral_vaults.get_mut(&dx_token).unwrap().take(take_amount);
            let normalized_amount = ceil(amount.checked_div(supply_index).unwrap(), divisibility);
            let underlying_bucket = underlying_pool.remove_liquity(dx_bucket);
            info!("amount:{}, take_amount:{}, normalized_amount:{}, underlying_bucket.amount:{}",amount, take_amount, normalized_amount, underlying_bucket.amount());
            self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", dx_amount.checked_sub(normalized_amount).unwrap());
            (underlying_bucket, cdp)
        }
        
        fn validate_withdraw_collateral(&self, dx_token: ResourceAddress, dx_amount: Decimal,
            borrow_token: ResourceAddress, borrow_price_in_xrd: Decimal, 
            collateral_underlying_price_in_xrd: Decimal, cdp_normalized_borrow: Decimal,
            withdraw_amount: Decimal
        ){
            let max_loan_amount = self.get_max_loan_amount(dx_token, dx_amount, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd, withdraw_amount);
            let borrow_pool = self.pools.get(&borrow_token).unwrap();
            let (_supply_index, borrow_index) = borrow_pool.get_current_index();
            let current_borrow_amount = cdp_normalized_borrow.checked_mul(borrow_index).unwrap();
            info!("current_borrow_amount:{}, max_loan_amount:{}, withdraw_amount:{}",  current_borrow_amount, max_loan_amount, withdraw_amount);
            assert!(max_loan_amount >= current_borrow_amount, "Insufficient remaining collateral.");
        }

        pub fn addition_collateral(&mut self, id: u64, bucket: Bucket){
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let dx_token = cdp_data.collateral_token;
            
            let dx_bucket = self.get_dx_bucket(cdp_data.collateral_token, dx_token, bucket);
            let dx_amount = dx_bucket.amount();
            self.put_collateral_vault(dx_bucket);
            self.update_cdp_data(cdp_data.is_stable, Decimal::ZERO, Decimal::ZERO, dx_amount,  Decimal::ZERO, Decimal::ZERO, cdp_id, cdp_data);

        }

        fn get_dx_bucket(&mut self, collateral_token: ResourceAddress, dx_token: ResourceAddress, bucket: Bucket)-> Bucket{
            let bucket_token = bucket.resource_address();
            let underlying_token = *self.deposit_asset_map.get(&dx_token).unwrap();
            assert!(collateral_token == bucket_token || underlying_token == bucket_token , "The addition of collateralized asset must match the current CDP collateral asset.");

            if bucket_token == collateral_token {
                bucket
            } else{
                let underlying_pool = self.pools.get_mut(&underlying_token).unwrap();
                underlying_pool.add_liquity(bucket)
            }
        }

        pub fn repay(&mut self, repay_bucket: Bucket, id: u64) -> (Bucket, Decimal){
            let cdp_id: NonFungibleLocalId = NonFungibleLocalId::integer(id);
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            assert_resource(&borrow_token, &repay_bucket.resource_address());
            
            let (bucket, payment_amount) = if cdp_data.is_stable {
                let (return_bucket, actual_repay_amount, repay_in_borrow) = self.repay_stable_to_pool(borrow_token, repay_bucket, cdp_data.borrow_amount, cdp_data.stable_rate, cdp_data.last_update_epoch, None);
                self.update_cdp_after_repay(&cdp_id, cdp_data, actual_repay_amount, repay_in_borrow, Decimal::ZERO, Decimal::ZERO);
                (return_bucket, actual_repay_amount)
            }
            else{
                let (return_bucket, actual_repay_amount, repay_normalized_amount) = self.repay_variable_to_pool(borrow_token, repay_bucket, cdp_data.normalized_borrow, None);
                self.update_cdp_after_repay(&cdp_id, cdp_data, actual_repay_amount, Decimal::ZERO, repay_normalized_amount, Decimal::ZERO);
                (return_bucket, actual_repay_amount)
            };

            (bucket, payment_amount)
        }

        fn repay_variable_to_pool(&mut self, borrow_token: ResourceAddress, repay_bucket: Bucket, cdp_normalized_borrow: Decimal, repay_opt: Option<Decimal>) -> (Bucket, Decimal, Decimal){
            let amount = repay_bucket.amount();
            let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();
            let (bucket, repay_normalized_amount) = borrow_pool.repay_variable(repay_bucket, cdp_normalized_borrow, repay_opt);
            let actual_repay_amount = amount.checked_sub(bucket.amount()).unwrap();
            (bucket, actual_repay_amount, repay_normalized_amount)
        }

        fn repay_stable_to_pool(&mut self, borrow_token: ResourceAddress, repay_bucket: Bucket, stable_borrow_amount: Decimal, stable_rate: Decimal, last_update_epoch: u64, repay_opt: Option<Decimal>) -> (Bucket, Decimal, Decimal){
            let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();
            let (bucket, actual_repay_amount, repay_in_borrow, _interest, _current_epoch_at) = borrow_pool.repay_stable(
                repay_bucket, stable_borrow_amount, stable_rate, last_update_epoch, repay_opt
            );
            (bucket, actual_repay_amount, repay_in_borrow)
        }

        pub fn liquidation(&mut self,
            debt_bucket: Bucket,
            debt_to_cover: Decimal,
            cdp_id: NonFungibleLocalId,
            borrow_price_in_xrd: Decimal, 
            underlying_token: ResourceAddress,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;
            let dx_amount = cdp_data.collateral_amount;

            let (actual_debt_to_liquidate,release_collateral_to_liqiudate) = self.get_liquidate_debt_and_collateral(
                borrow_price_in_xrd, collateral_underlying_price_in_xrd, debt_to_cover,
                borrow_token, underlying_token.clone(), cdp_data.borrow_amount, cdp_data.normalized_borrow, cdp_data.collateral_amount, cdp_data.is_stable,
                cdp_data.stable_rate, cdp_data.last_update_epoch
            );
            info!("actual_debt_to_liquidate:{}, release_collateral_to_liqiudate:{}", actual_debt_to_liquidate, release_collateral_to_liqiudate);

            let repay_amount = debt_bucket.amount();
            assert!(repay_amount >= actual_debt_to_liquidate, "the debt bucket does not cover to debt of the CDP.");
            
            let (bucket, actual_repay_amount) = if cdp_data.is_stable{
                let (return_bucket, actual_repay_amount, repay_in_borrow) = self.repay_stable_to_pool(borrow_token, debt_bucket, cdp_data.borrow_amount, cdp_data.stable_rate, cdp_data.last_update_epoch, Some(actual_debt_to_liquidate));
                info!("stable debt: actual_repay_amount:{}, bucket:{}", actual_repay_amount, return_bucket.amount());
                self.update_cdp_after_repay(&cdp_id, cdp_data, actual_repay_amount, repay_in_borrow, Decimal::ZERO, Decimal::ZERO);
                (return_bucket, actual_repay_amount)
            }
            else{
                let (return_bucket, actual_repay_amount, repay_normalized_amount) = self.repay_variable_to_pool(borrow_token, debt_bucket, cdp_data.normalized_borrow, Some(actual_debt_to_liquidate));
                info!("variable debt: repay_normalized_amount:{}, bucket:{}, actual_repay_amount:{}", repay_normalized_amount, return_bucket.amount(), actual_repay_amount);
                self.update_cdp_after_repay(&cdp_id, cdp_data, actual_debt_to_liquidate, Decimal::ZERO, repay_normalized_amount, Decimal::ZERO);
                (return_bucket, actual_repay_amount)
            };
            assert!(actual_repay_amount == actual_debt_to_liquidate, "The actual repay amount dose not matches debt to liquidate.");

            info!("debt_bucket:{}", bucket.amount());
            let underlying_pool = self.pools.get_mut(&underlying_token).unwrap();
            let mut vault = self.collateral_vaults.get_mut(&dx_token).unwrap();
            info!("underlying:{}, dx:{}, dx_vault:{}", Runtime::bech32_encode_address(underlying_token), Runtime::bech32_encode_address(dx_token),vault.amount());
            let release_underlying_bucket = underlying_pool.remove_liquity(vault.take(release_collateral_to_liqiudate));
            info!("underlying(collateral) amount:{}", release_underlying_bucket.amount());
            self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", dx_amount.checked_sub(release_collateral_to_liqiudate).unwrap());
            (release_underlying_bucket, bucket)

        }

        pub fn borrow_flashloan(&mut self, res_addr: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            assert!(self.pools.get(&res_addr).is_some(), "unknow token resource address.");
            let pool = self.pools.get_mut(&res_addr).unwrap();
            let bucket = pool.borrow_flashloan(amount);
            let fee = bucket.amount().checked_mul(pool.get_flashloan_fee_ratio()).unwrap();
            self.transient_id_counter += 1;
            let data = FlashLoanData{
                amount: bucket.amount(),
                res_addr,
                fee
            };
            let flashloan_nft = self.transient_nft_res_mgr.mint_non_fungible::<FlashLoanData>(&NonFungibleLocalId::integer(self.transient_id_counter), data);
            (bucket, flashloan_nft)
        }

        pub fn repay_flashloan(&mut self, repay_bucket: Bucket, flashloan: Bucket) -> Bucket{
            let underlying = repay_bucket.resource_address();
            assert!(self.pools.get(&underlying).is_some(), "unknow token resource address.");

            let flashloan_id : NonFungibleLocalId = flashloan.as_non_fungible().non_fungible_local_id();
            let flashloan_data = self.transient_nft_res_mgr.get_non_fungible_data::<FlashLoanData>(&flashloan_id);
            assert!(
                underlying == flashloan_data.res_addr 
                && repay_bucket.amount() >= flashloan_data.amount.checked_add(flashloan_data.fee).unwrap(),
                 "The token resource address does not matches!"
                );
            
            self.transient_nft_res_mgr.burn(flashloan);
            let pool = self.pools.get_mut(&underlying).unwrap();
            pool.repay_flashloan(repay_bucket, flashloan_data.amount, flashloan_data.fee)
        }



        pub fn withdraw_insurance(&mut self, underlying_token_addr: ResourceAddress, amount: Decimal) -> Bucket{
            assert!(self.pools.get(&underlying_token_addr).is_some(), "unknow token resource address.");
            let pool = self.pools.get_mut(&underlying_token_addr).unwrap();
            pool.withdraw_insurance(amount)
        }

        pub fn get_underlying_token(&self, dx_token: ResourceAddress) -> ResourceAddress{
            assert!(self.deposit_asset_map.get(&dx_token).is_some(), "unknow resource address.");
            self.deposit_asset_map.get(&dx_token).unwrap().clone()
        }

        pub fn get_cdp_resource_address(&self, cdp_id: NonFungibleLocalId)->(ResourceAddress, ResourceAddress){
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;

            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            (borrow_token, underlying_token.clone())
        }

        fn get_liquidate_debt_and_collateral(&self,
            debt_price: Decimal,
            collateral_underlying_price: Decimal,
            debt_to_cover: Decimal,
            borrow_token: ResourceAddress,
            underlying_token: ResourceAddress,
            borrow_amount: Decimal,
            normalized_borrow: Decimal,
            collateral_amount: Decimal,
            is_stable: bool,
            stable_rate: Decimal,
            last_update_epoch: u64
        ) -> (Decimal, Decimal){
            let underlying_pool = self.pools.get(&underlying_token).unwrap();
            let debt_pool = self.pools.get(&borrow_token).unwrap();
            let underlying_state = self.states.get(&underlying_token).unwrap();
            let liquidation_threshold = underlying_state.liquidation_threshold;
            let liquidation_bonus = underlying_state.liquidation_bonus;

            let underlying_amount = underlying_pool.get_redemption_value(collateral_amount);
            let debt_amount = if is_stable {
                debt_pool.get_stable_interest(borrow_amount, last_update_epoch, stable_rate)
            }else{
                debt_pool.get_variable_interest(normalized_borrow)
            };
            let underlying_value = underlying_amount.checked_mul(collateral_underlying_price).unwrap();
            let health_factor = underlying_value.checked_mul(liquidation_threshold).unwrap()
            .checked_div(debt_amount.checked_mul(debt_price).unwrap()).unwrap();
            assert!(health_factor <= Decimal::ONE, "Health factor is not below the threshold");

            let collateral_to_underlying_index = underlying_amount.checked_div(collateral_amount).unwrap();
            let max_to_liquidate = precent_mul(debt_amount, self.close_factor_percent);
            info!("debt_amount: {}, max_to_liquidate:{}",debt_amount, max_to_liquidate);
            let mut actual_to_liquidate = if debt_to_cover.is_positive() && max_to_liquidate > debt_to_cover {debt_to_cover} else{max_to_liquidate};
            // debt.amount * debt.price * (1+liquidation_bonus) / underlying.price
            let mut underlying_to_liquidate = actual_to_liquidate.checked_mul(debt_price).unwrap().checked_mul(
                Decimal::ONE.checked_add(liquidation_bonus).unwrap()
            ).unwrap().checked_div(collateral_underlying_price).unwrap();
            info!("underlying_to_liquidate:{}, actual_to_liquidate:{} price:{}, bonus:{}, index:{}", underlying_to_liquidate, actual_to_liquidate, debt_price, liquidation_bonus, collateral_to_underlying_index);

            if underlying_to_liquidate > underlying_amount {
                underlying_to_liquidate = underlying_amount;
                actual_to_liquidate = underlying_value.checked_div(
                    debt_price.checked_mul(Decimal::ONE.checked_add(liquidation_bonus).unwrap()).unwrap()
                ).unwrap();
                info!("underlying_to_liquidate:{}, underlying_amount:{} actual_to_liquidate:{}", underlying_to_liquidate, underlying_amount, actual_to_liquidate);
            };

            (
                actual_to_liquidate, 
                underlying_to_liquidate.checked_div(collateral_to_underlying_index).unwrap()
            )
            
        }

        fn update_cdp_after_repay(&mut self, 
            cdp_id: &NonFungibleLocalId,
            cdp_data: CollateralDebtPosition,
            repay_amount: Decimal,
            delta_borrow: Decimal,
            delta_normalized_borrow: Decimal,
            delta_collateral: Decimal
        ){
            info!("total_repay:{}-{}", cdp_data.total_repay, repay_amount);
            self.cdp_res_mgr.update_non_fungible_data(cdp_id, "total_repay", cdp_data.total_repay.checked_add(repay_amount).unwrap());
            if !cdp_data.is_stable && delta_normalized_borrow != Decimal::ZERO{
                info!("normalized_borrow:{}-{}", cdp_data.normalized_borrow, delta_normalized_borrow);
                self.cdp_res_mgr.update_non_fungible_data(cdp_id, "normalized_borrow", cdp_data.normalized_borrow.checked_sub(delta_normalized_borrow).unwrap());
            }

            if cdp_data.is_stable && delta_borrow != Decimal::ZERO{
                let new_borrow_amount = cdp_data.borrow_amount - delta_borrow;
                info!("borrow_amount:{}-{}", cdp_data.borrow_amount, delta_borrow);
                self.cdp_res_mgr.update_non_fungible_data(cdp_id, "borrow_amount", new_borrow_amount);
                if new_borrow_amount == Decimal::ZERO {
                    self.cdp_res_mgr.update_non_fungible_data(cdp_id, "stable_rate", Decimal::ZERO);
                }
                self.cdp_res_mgr.update_non_fungible_data(cdp_id, "last_update_epoch", Runtime::current_epoch().number());
            }

            if delta_collateral != Decimal::ZERO{
                info!("collateral_amount:{}|{}", cdp_data.collateral_amount, delta_collateral);
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", cdp_data.collateral_amount + delta_collateral);
            }
        }

        fn update_cdp_data(&mut self,
            is_stable: bool,
            delta_borrow: Decimal,
            interest: Decimal,
            delta_collateral: Decimal,
            delta_normalized_borrow: Decimal,
            cdp_avg_rate:Decimal,
            cdp_id: NonFungibleLocalId,
            data: CollateralDebtPosition
        ){
            if delta_borrow != Decimal::ZERO || interest != Decimal::ZERO {
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "total_borrow", data.total_borrow + delta_borrow);
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "borrow_amount", data.borrow_amount + delta_borrow + interest);
            }
            if delta_normalized_borrow != Decimal::ZERO {
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "normalized_borrow", data.normalized_borrow + delta_normalized_borrow);
            }
            if delta_collateral != Decimal::ZERO {
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", data.collateral_amount + delta_collateral);
            }
            if is_stable {
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "stable_rate", cdp_avg_rate);
                self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "last_update_epoch", Runtime::current_epoch().number());
            }
            
        }

        fn new_cdp(&mut self,
            dx_addr: ResourceAddress,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            collateral_amount: Decimal,
            borrow_normalized_amount: Decimal,
            cdp_avg_rate: Decimal,
            is_stable: bool
        ) -> Bucket{
            let epoch_at = if is_stable {Runtime::current_epoch().number()} else{0u64};
            let data = CollateralDebtPosition{
                collateral_token: dx_addr.clone(),
                total_borrow: borrow_amount,
                total_repay: Decimal::ZERO,
                normalized_borrow: borrow_normalized_amount,
                last_update_epoch: epoch_at,
                stable_rate: cdp_avg_rate,
                collateral_amount,
                borrow_amount,
                is_stable,
                borrow_token
            };
            self.cdp_id_counter += 1;
            self.cdp_res_mgr.mint_non_fungible(&NonFungibleLocalId::integer(self.cdp_id_counter), data)
        }

        fn put_collateral_vault(&mut self, bucket: Bucket){
            let res_addr = bucket.resource_address();
            if self.collateral_vaults.get(&res_addr).is_some(){
                self.collateral_vaults.get_mut(&res_addr).unwrap().put(bucket);
            }
            else{
                self.collateral_vaults.insert(res_addr, Vault::with_bucket(bucket));
            }
        }

        // fn get_token(&self, dx_token: ResourceAddress) -> (ResourceAddress, Decimal, Decimal){
        //     let underlying_token = *self.deposit_asset_map.get(&dx_token).unwrap();
        //     let underlying_pool = self.get_
        //     (underlying_token, self)
        // }

        ///
        /// Calculate the maximum loan amount based on the provided parameters.                                         
        /// |   borrow   |   collateral      |   price(base/quote) | stage                                          |
        /// | ---------- | ----------------- | ------------------- | ---------------------------------------------- |
        /// | XRD        | USDC              | XRD/USDC            | borrow=price1.base, collateral=price1.quote    |
        /// | USDT       | USDC              | XRD/USDC, XRD/USDT  | borrow=price1.quote, collateral=price2.quote   |
        /// | USDT       | XRD               | XRD/USDT            | borrow=price1.quote, collateral=price1.base    |
        /// | USDC       | XRD               | XRD/USDC            | borrow=price1.quote, collateral=price1.base    |
        ///
        fn get_max_loan_amount(&self,
            dx_token: ResourceAddress,
            dx_amount: Decimal,
            borrow_token: ResourceAddress,
            borrow_price_in_xrd: Decimal,
            collateral_price_in_xrd: Decimal,
            remove_amount: Decimal
        ) -> Decimal {
            let collateral_token = *self.deposit_asset_map.get(&dx_token).unwrap();
            let underlying_pool = self.pools.get(&collateral_token).unwrap();
            let underlying_state = self.states.get(&collateral_token).unwrap();
            let underlying_amount = underlying_pool.get_redemption_value(dx_amount);
            let amount = underlying_amount.checked_sub(remove_amount).unwrap();
            let ltv = underlying_state.ltv;
            assert!(ltv > Decimal::ZERO, "Loan to Value(LTV) of the collateral asset equals ZERO!");
            
            info!(
                "get_max_loan_amount: {}|{},{}*{}, price:{}/{}, underlying_amount:{}, remove_amount:{}",
                Runtime::bech32_encode_address(dx_token),dx_amount, amount,ltv,borrow_price_in_xrd,collateral_price_in_xrd, underlying_amount, remove_amount
            );


            let divisibility = get_divisibility(borrow_token);
            if ltv.is_zero() || divisibility.is_none() {
                return Decimal::ZERO;
            }

            if borrow_token == XRD && collateral_price_in_xrd.is_positive(){
                return floor(collateral_price_in_xrd.checked_mul(amount).unwrap()
                .checked_mul(ltv).unwrap()
                .checked_div(borrow_price_in_xrd).unwrap(), divisibility.unwrap());
            }
            
            if borrow_token != XRD && collateral_token != XRD {
                if borrow_price_in_xrd.is_positive() && collateral_price_in_xrd.is_positive() {
                    return floor(collateral_price_in_xrd.checked_mul(amount).unwrap()
                    .checked_mul(ltv).unwrap()
                    .checked_div(borrow_price_in_xrd).unwrap(), divisibility.unwrap());
                }
            }
            
            if collateral_token == XRD && borrow_price_in_xrd.is_positive() {
                return floor(
                    collateral_price_in_xrd.checked_mul(amount).unwrap()
                    .checked_mul(ltv).unwrap()
                    .checked_div(borrow_price_in_xrd).unwrap(),
                    divisibility.unwrap()
                );
            }

            Decimal::ZERO
            
        }

    }
}