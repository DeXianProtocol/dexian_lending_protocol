use scrypto::prelude::*;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
use crate::utils::*;


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
        },
        methods{
            new_pool => restrict_to:[operator, OWNER];
            withdraw_insurance => restrict_to: [operator, OWNER];

            supply => PUBLIC;
            withdraw => PUBLIC;
            borrow_variable => PUBLIC;
            borrow_stable => PUBLIC;
            extend_borrow => PUBLIC;
            withdraw_collateral => PUBLIC;
            repay => PUBLIC;
            addition_collateral => PUBLIC;
            get_underlying_token => PUBLIC;
            get_cdp_resource_address => PUBLIC;
        }
    }

    struct CollateralDebtManager{
        price_oracle: Global<PriceOracle>,
        // Lend Pool of each asset in the lending pool. I.E.: XRD ==> LendResourcePool(XRD)
        pools: HashMap<ResourceAddress, Global<LendResourcePool>>,
        //Status of each asset in the lending pool, I.E.: XRD ==> AssetState(XRD)
        states: HashMap<ResourceAddress, AssetState>,
        // address map for supply token(K) and deposit token(V), I.E. dxXRD --> XRD
        deposit_asset_map: HashMap<ResourceAddress, ResourceAddress>,
        // vault for each collateral asset(supply token), I.E. dxXRD ==> Vault(dxXRD)
        collateral_vaults: HashMap<ResourceAddress, Vault>,
        // CDP token define
        cdp_res_mgr: ResourceManager,
        // CDP id counter
        cdp_id_counter: u64,
        self_cmp_addr: ComponentAddress
    }

    impl CollateralDebtManager{

        pub fn instantiate(admin_rule: AccessRule, pool_mgr_rule: AccessRule, price_oracle: Global<PriceOracle>)->(Global<CollateralDebtManager>, ResourceAddress){
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
            
            let component = Self{
                pools: HashMap::new(),
                states: HashMap::new(),
                deposit_asset_map: HashMap::new(),
                collateral_vaults: HashMap::new(),
                self_cmp_addr: address,
                price_oracle,
                cdp_id_counter: 0u64,
                cdp_res_mgr,
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .with_address(address_reservation)
            .roles(roles!{
                admin => admin_rule.clone();
                operator => pool_mgr_rule.clone();
            }
            )
            .globalize();
            (component, cdp_res_mgr.address())
        }

        pub fn new_pool(&mut self, 
            underlying_token_addr: ResourceAddress,
            interest_model: InterestModel,
            interest_model_cmp_addr: ComponentAddress,
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal,
            admin_rule: AccessRule,
        ) -> ResourceAddress{
            let pool_mgr_rule = rule!(require(global_caller(self.self_cmp_addr)));
            let (lend_res_pool, dx_token_addr) = LendResourcePool::instantiate(
                underlying_token_addr, 
                interest_model_cmp_addr,
                interest_model.clone(),
                insurance_ratio,
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
            self.pools.entry(underlying_token_addr).or_insert(lend_res_pool);
            self.states.entry(underlying_token_addr).or_insert(asset_state);
            self.collateral_vaults.entry(dx_token_addr).or_insert(Vault::new(dx_token_addr));
            self.deposit_asset_map.entry(dx_token_addr).or_insert(underlying_token_addr);
            dx_token_addr
        }

        pub fn supply(&mut self, bucket: Bucket) -> Bucket{
            let supply_res_addr = bucket.resource_address();
            assert!(self.pools.contains_key(&supply_res_addr), "There is no pool of funds corresponding to the assets!");
            let lending_pool = self.pools.get_mut(&supply_res_addr).unwrap();
            lending_pool.add_liquity(bucket)
        }

        pub fn withdraw(&mut self, bucket: Bucket) -> Bucket{
            let dx_token = bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_token), "the token has not supported!");
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let lending_pool = self.pools.get_mut(&underlying_token).unwrap();
            lending_pool.remove_liquity(bucket)
        }

        pub fn borrow_variable(&mut self,
            dx_bucket: Bucket,
            underlying_token: ResourceAddress,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            let dx_token = dx_bucket.resource_address();
            assert!(dx_bucket.amount() > Decimal::ZERO, "the bucket is empty!");
            
            let asset_state = self.states.get(&underlying_token).unwrap();
            let ltv = asset_state.ltv;
            assert!(ltv > Decimal::ZERO, "Loan to Value(LTV) of the collateral asset equals ZERO!");
            
            let dx_amount = dx_bucket.amount();
            let collateral_pool = self.pools.get(&underlying_token).unwrap();
            let underlying_token_amount = collateral_pool.get_redemption_value(dx_amount);

            let max_loan_amount = self.get_max_loan_amount(underlying_token.clone(), underlying_token_amount, ltv, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            assert_amount(max_loan_amount, borrow_amount);

            self.put_collateral_vault(dx_bucket);
            let lending_pool = self.pools.get_mut(&borrow_token).unwrap();
            let (borrow_bucket, borrow_normalized_amount) = lending_pool.borrow_variable(borrow_amount);
            //mint cdp
            let cdp_bucket = self.new_cdp(dx_token, borrow_token, borrow_amount, dx_amount, borrow_normalized_amount, Decimal::ZERO, false);
            (borrow_bucket, cdp_bucket)
        }

        pub fn borrow_stable(&mut self,
            dx_bucket: Bucket,
            underlying_token: ResourceAddress,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            borrow_price_in_xrd: Decimal,
            collateral_underlying_price_in_xrd: Decimal
        ) -> (Bucket, Bucket){
            let dx_token = dx_bucket.resource_address();
            assert!(dx_bucket.amount() > Decimal::ZERO, "the bucket is empty!");
            
            let asset_state = self.states.get(&underlying_token).unwrap();
            let ltv = asset_state.ltv;
            assert!(ltv > Decimal::ZERO, "Loan to Value(LTV) of the collateral asset equals ZERO!");
            
            let dx_amount = dx_bucket.amount();
            let collateral_pool = self.pools.get(&underlying_token).unwrap();
            let underlying_token_amount = collateral_pool.get_redemption_value(dx_amount);

            let max_loan_amount = self.get_max_loan_amount(underlying_token.clone(), underlying_token_amount, ltv, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            assert_amount(max_loan_amount, borrow_amount);
            
            self.put_collateral_vault(dx_bucket);
            let lending_pool = self.pools.get_mut(&borrow_token).unwrap();
            let (_variable_rate,stable_rate,_supply_rate) = lending_pool.get_interest_rate();
            let borrow_bucket = lending_pool.borrow_stable(borrow_amount, stable_rate);
            
            //mint cdp
            let cdp_bucket = self.new_cdp(dx_token, borrow_token, borrow_amount, dx_amount, Decimal::ZERO, stable_rate, true);
            (borrow_bucket, cdp_bucket)
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

            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let underlying_pool = self.pools.get(underlying_token).unwrap();
            let underlying_state = self.states.get(underlying_token).unwrap();
            let underlying_token_amount = underlying_pool.get_redemption_value(dx_amount);
            let ltv = underlying_state.ltv;
            let max_loan_amount = self.get_max_loan_amount(underlying_token.clone(), underlying_token_amount, ltv, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd);

            let mut cdp_avg_rate = Decimal::ZERO;
            let mut interest = Decimal::ZERO;
            let mut delta_normalized_amount = Decimal::ZERO;
            let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();
            let borrow_bucket: Bucket = if cdp_data.is_stable {
                interest = borrow_pool.get_stable_interest(cdp_data.borrow_amount, cdp_data.last_update_epoch, cdp_data.stable_rate);
                let borrow_intent = cdp_data.borrow_amount.checked_add(interest).unwrap().checked_add(amount).unwrap();
                assert_amount(borrow_intent, max_loan_amount);
                
                let (_variable_rate, stable_rate, _supply_rate)  = borrow_pool.get_interest_rate();
                let borrow_bucket = borrow_pool.borrow_stable(amount, stable_rate);
                cdp_avg_rate = get_weight_rate(cdp_data.borrow_amount.checked_add(interest).unwrap(), cdp_data.stable_rate, amount, stable_rate);
                let increase_amount = amount + interest;
                borrow_bucket
            }
            else{
                let (_, current_borrow_index) = borrow_pool.get_current_index();
                let exist_borrow = cdp_data.normalized_borrow * current_borrow_index;
                let borrow_intent = exist_borrow.checked_add(amount).unwrap();
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

            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let underlying_pool = self.pools.get(underlying_token).unwrap();
            let (supply_index, _borrow_index) = underlying_pool.get_current_index();
            let underlying_state = self.states.get(underlying_token).unwrap();
            let underlying_token_amount = underlying_pool.get_redemption_value(dx_amount);
            let underlying_reserve_amount = underlying_token_amount.checked_sub(amount).unwrap();
            let ltv = underlying_state.ltv;

            let max_loan_amount = self.get_max_loan_amount(underlying_token.clone(), underlying_reserve_amount, ltv, borrow_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            let borrow_pool = self.pools.get(&borrow_token).unwrap();
            let (_supply_index, borrow_index) = borrow_pool.get_current_index();
            let current_borrow_amount = cdp_data.normalized_borrow.checked_mul(borrow_index).unwrap();
            assert_amount(current_borrow_amount, max_loan_amount);
            
            let divisibility = get_divisibility(dx_token).unwrap();
            let take_amount = floor(amount.checked_div(supply_index).unwrap(), divisibility);
            let dx_bucket = self.collateral_vaults.get_mut(&dx_token).unwrap().take(take_amount);
            let normalized_amount = ceil(amount.checked_div(supply_index).unwrap(), divisibility);
            let underlying_bucket = underlying_pool.remove_liquity(dx_bucket);
            self.cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", dx_amount.checked_sub(normalized_amount));
            (underlying_bucket, cdp)
        }

        pub fn addition_collateral(&mut self, id: u64, bucket: Bucket){
            let bucket_token = bucket.resource_address();
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let dx_token = cdp_data.collateral_token;
            
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            assert!(cdp_data.collateral_token == bucket_token || underlying_token == &bucket_token , "The addition of collateralized asset must match the current CDP collateral asset.");

            let dx_bucket = if bucket_token == cdp_data.collateral_token {
                bucket
            } else{
                let underlying_pool = self.pools.get_mut(&underlying_token).unwrap();
                underlying_pool.add_liquity(bucket)
            };
            let dx_amount = dx_bucket.amount();
            self.put_collateral_vault(dx_bucket);
            self.update_cdp_data(cdp_data.is_stable, Decimal::ZERO, Decimal::ZERO, dx_amount,  Decimal::ZERO, Decimal::ZERO, cdp_id, cdp_data);
        }

        pub fn repay(&mut self, mut repay_bucket: Bucket, id: u64) -> Bucket{
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            assert_resource(&borrow_token, &repay_bucket.resource_address());

            let mut repay_amount = repay_bucket.amount();
            let mut repay_in_borrow = Decimal::ZERO;
            let mut normalized_amount = Decimal::ZERO;
            let borrow_pool = self.pools.get_mut(&borrow_token).unwrap();

            if cdp_data.is_stable {
                let (bucket, actual_repay_amount, repay_in_borrow, interest, current_epoch_at) = borrow_pool.repay_stable(
                    repay_bucket, cdp_data.borrow_amount, cdp_data.stable_rate, cdp_data.last_update_epoch
                );
                //TODO: update cdp
            }
            else{
                let bucket = borrow_pool.repay_variable(repay_bucket);
            }

            Bucket::new(borrow_token)
        }

        pub fn withdraw_insurance(&mut self, underlying_token_addr: ResourceAddress, amount: Decimal) -> Bucket{
            assert!(self.pools.contains_key(&underlying_token_addr), "unknow token resource address.");
            let pool = self.pools.get_mut(&underlying_token_addr).unwrap();
            pool.withdraw_insurance(amount)
        }

        pub fn get_underlying_token(&self, dx_token: ResourceAddress) -> ResourceAddress{
            assert!(self.deposit_asset_map.contains_key(&dx_token), "unknow resource address.");
            self.deposit_asset_map.get(&dx_token).unwrap().clone()
        }

        pub fn get_cdp_resource_address(&self, cdp_id: NonFungibleLocalId)->(ResourceAddress, ResourceAddress){
            let cdp_data = self.cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;

            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            (borrow_token, underlying_token.clone())
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
            if self.collateral_vaults.contains_key(&res_addr){
                self.collateral_vaults.get_mut(&res_addr).unwrap().put(bucket);
            }
            else{
                self.collateral_vaults.insert(res_addr, Vault::with_bucket(bucket));
            }
        }

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
            collateral_token: ResourceAddress,
            amount: Decimal,
            ltv: Decimal,
            borrow_token: ResourceAddress,
            borrow_price_in_xrd: Decimal,
            collateral_price_in_xrd: Decimal
        ) -> Decimal {
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