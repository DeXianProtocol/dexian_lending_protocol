use scrypto::prelude::*;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::{def_interest_model::DefInterestModel, InterestModel};
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
            comp_owner => updatable_by: [];
            admin => updatable_by: [];
        },
        methods{
            new_pool => restrict_to:[admin, OWNER];
            // get_asset_state => PUBLIC;
            // put_collateral_vault => PUBLIC;
        }
    }

    struct CollateralDebtManager{
        price_oracle: Global<PriceOracle>,
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
        owner_role: OwnerRole,
        pool_mgr_rule: AccessRule,
    }

    impl CollateralDebtManager{

        pub fn instantiate(owner_role: OwnerRole, pool_mgr_rule: AccessRule, price_oracle: Global<PriceOracle>)->(Global<CollateralDebtManager>, ResourceAddress){
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
                price_oracle,
                cdp_id_counter: 0u64,
                cdp_res_mgr,
                owner_role,
                pool_mgr_rule
            }.instantiate()
            .prepare_to_globalize(owner_role)
            .with_address(address_reservation)
            // .roles(

            // ).globalize();
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
            collateral_token: ResourceAddress
        ) -> ResourceAddress{
            let (lend_res_pool, dx_token_addr) = LendResourcePool::instantiate(
                underlying_token_addr, 
                interest_model_cmp_addr,
                interest_model,
                insurance_ratio,
                self.owner_role,
                self.pool_mgr_rule
                );
            let asset_state = AssetState{
                interest_model,
                collateral_token,
                ltv,
                liquidation_threshold,
                liquidation_bonus
            };
            self.states.entry(underlying_token_addr).or_insert(asset_state);
            self.collateral_vaults.entry(collateral_token).or_insert(Vault::new(collateral_token));
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
            assert!(self.deposit_asset_map.contains_key(&dx_token), "unsupported the token!");
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let lending_pool = self.pools.get_mut(&underlying_token).unwrap();
            lending_pool.remove_liquity(bucket)
        }

        pub fn borrow_variable(&mut self,
            dx_bucket: Bucket,
            borrow_token: ResourceAddress,
            borrow_amount: Decimal,
            price1: String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (Bucket, Bucket){
            let dx_token = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_token) && dx_bucket.amount() > Decimal::ZERO, "unsupported the token or bucket is empty!");
            
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();
            let asset_state = self.states.get(&underlying_token).unwrap();
            let ltv = asset_state.ltv;
            assert!(ltv > Decimal::ZERO, "Loan to Value(LTV) of the collateral asset equals ZERO!");
            
            let lending_pool = self.pools.get_mut(&underlying_token).unwrap();
            let amount = lending_pool.get_redemption_value(dx_bucket.amount());

            let max_loan_amount = self.get_max_loan_amount(underlying_token.clone(), amount, ltv, borrow_token, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2);
            assert_amount(max_loan_amount, borrow_amount);

            //mint cdp            
            let borrow_bucket = lending_pool.borrow_variable(borrow_amount);
            (borrow_bucket, borrow_bucket)
        }

        fn put_collateral_vault(&mut self, bucket: Bucket){
            let res_addr = bucket.resource_address();
            self.collateral_vaults.entry(res_addr).and_modify(|vault|{
                vault.put(bucket);
            }).or_insert(Vault::with_bucket(bucket));
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
            price1: String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> Decimal {
            let divisibility = get_divisibility(borrow_token);
            if ltv.is_zero() || divisibility.is_none() {
                return Decimal::ZERO;
            }

            if borrow_token == XRD && collateral_token == quote1 {
                let collateral_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1, timestamp1, signature1);
                let borrow_price_in_xrd = Decimal::ONE;
                return floor(collateral_price_in_xrd.checked_mul(amount).unwrap()
                .checked_mul(ltv).unwrap()
                .checked_div(borrow_price_in_xrd).unwrap(), divisibility.unwrap());
            }
            
            if borrow_token == quote1 && quote2.is_some() && collateral_token == quote2.unwrap(){
                let collateral_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote2.unwrap(), price2.unwrap(), timestamp2.unwrap(), signature2.unwrap());
                let borrow_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1, timestamp1, signature1);
                if borrow_price_in_xrd.is_positive() {
                    return floor(collateral_price_in_xrd.checked_mul(amount).unwrap()
                    .checked_mul(ltv).unwrap()
                    .checked_div(borrow_price_in_xrd).unwrap(), divisibility.unwrap());
                }
            }
            
            if borrow_token == quote1 && collateral_token == XRD {
                let collateral_price_in_xrd = Decimal::ONE;
                let borrow_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1, timestamp1, signature1);
                if borrow_price_in_xrd.is_positive() {
                    return floor(collateral_price_in_xrd.checked_mul(amount).unwrap()
                    .checked_mul(ltv).unwrap()
                    .checked_div(borrow_price_in_xrd).unwrap(), divisibility.unwrap());
                }
            }

            Decimal::ZERO
            
        }

    }
}