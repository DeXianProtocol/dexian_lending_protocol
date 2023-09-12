
use scrypto::prelude::*;
use crate::oracle::oracle::PriceOracle;
use crate::def_interest_model::InterestModel;
use crate::def_interest_model::def_interest_model::DefInterestModel;

/**
* About 15017 epochs in a year, assuming 35 minute each epoch.
* https://learn.radixdlt.com/article/how-long-is-an-epoch-on-radix
* There is no exact timestamp at the moment, so for the time being the period of each epoch (35 minutes) is used for calculation.
**/
const EPOCH_OF_YEAR: u64 = 15017;

#[derive(ScryptoSbor, NonFungibleData)]
pub struct CollateralDebtPosition{
    pub borrow_token: ResourceAddress,
    pub collateral_token: ResourceAddress,

    #[mutable]
    pub is_stable: bool,
    #[mutable]
    pub total_borrow: Decimal,
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


///asset state
#[derive(ScryptoSbor)]
struct AssetState{
    pub def_interest_model: Global<DefInterestModel>,
    pub interest_model: InterestModel,
    // the liquidity index
    pub supply_index: Decimal,
    // the borrow index
    pub borrow_index: Decimal,
    // the current borrow annual interest rate
    pub borrow_interest_rate: Decimal,
    // the current supply annual interest rate
    pub supply_interest_rate: Decimal,
    // the insurance funding for current asset.
    pub insurance_balance: Decimal,
    // the ratio for current asset insurance funding
    pub insurance_ratio: Decimal,
    // LP token of asset
    pub token: ResourceAddress,
    // normalized total borrow.
    pub normalized_total_borrow: Decimal,
    // loan to value
    pub ltv: Decimal,
    // liquidation threshold
    pub liquidation_threshold: Decimal,
    //TODO: flash loan
    // pub flash_loan_fee: Decimal,
    // bonus for liquidator
    pub liquidation_bonus: Decimal,
    // last update timestamp
    pub last_update_epoch: u64,
    // last update for stable lending
    pub stable_borrow_last_update: u64,
    // total amount for stable rate lending
    pub stable_borrow_amount: Decimal,
    // weight average rate
    pub stable_avg_rate: Decimal,
}

impl AssetState {
        
    fn update_index(&mut self) {
        let delta_epoch = Runtime::current_epoch().number() - self.last_update_epoch;
        if delta_epoch > 0u64 {
            let (current_supply_index, current_borrow_index) = self.get_current_index();
            
            // get the total equity value
            let normalized_borrow: Decimal = self.normalized_total_borrow;
            let token_res_mgr = ResourceManager::from_address(self.token);
            let normalized_supply: Decimal = token_res_mgr.total_supply().unwrap();

            // interest = equity value * (current index value - starting index value)
            let recent_variable_interest = normalized_borrow * (current_borrow_index - self.borrow_index);
            let delta_epoch_year = Decimal::from(delta_epoch) / Decimal::from(EPOCH_OF_YEAR);
            let recent_stable_interest = self.stable_borrow_amount * self.stable_avg_rate * delta_epoch_year;
            let recent_supply_interest = normalized_supply * (current_supply_index - self.supply_index);

            // the interest rate spread goes into the insurance pool
            self.insurance_balance += recent_variable_interest + recent_stable_interest - recent_supply_interest;

            debug!("update_index({}), borrow_index:{}, current:{}, supply_index:{}, current:{}, stable:{}, stable_avg_rate:{}", self.token.to_hex(), self.borrow_index, current_borrow_index, self.supply_index, current_supply_index, self.stable_borrow_amount, self.stable_avg_rate);
            self.supply_index = current_supply_index;
            self.borrow_index = current_borrow_index;
            self.last_update_epoch = Runtime::current_epoch().number();

        }
    }

    pub fn get_current_index(&self) -> (Decimal, Decimal){
        let delta_epoch = Runtime::current_epoch().number() - self.last_update_epoch;
        let delta_epoch_year = Decimal::from(delta_epoch) / Decimal::from(EPOCH_OF_YEAR);
        let delta_borrow_interest_rate = self.borrow_interest_rate * delta_epoch_year;
        let delta_supply_interest_rate = self.supply_interest_rate * delta_epoch_year;

        (
            self.supply_index * (Decimal::ONE + delta_supply_interest_rate),
            self.borrow_index * (Decimal::ONE + delta_borrow_interest_rate)
        )

    }

    pub fn get_interest_rates(&self, change_amount: Decimal, is_stable: bool) -> (Decimal, Decimal, Decimal){
        let (current_supply_index, current_borrow_index) = self.get_current_index();

        // This supply could be equal to zero.
        let supply = self.get_total_supply_with_index(current_supply_index);
        let mut variable_borrow = self.get_total_borrow_with_index(current_borrow_index);
        let mut stable_borrow = self.get_total_stable_borrow();
        if is_stable {
            stable_borrow += change_amount;
        }
        else{
            variable_borrow += change_amount;
        }
        
        let (variable_borrow_rate, stable_borrow_rate, supply_rate) = self.calc_interest_rate(variable_borrow, stable_borrow, supply);
        (variable_borrow_rate, stable_borrow_rate, supply_rate)
    }

    pub fn get_current_supply_borrow(&self) -> (Decimal, Decimal){
        let (supply_index, borrow_index) = self.get_current_index();
        (
            self.get_total_supply_with_index(supply_index),
            self.get_total_borrow_with_index(borrow_index) + self.get_total_stable_borrow()
        )
    }

    pub fn get_ltv_and_liquidation_threshold(&self) -> (Decimal, Decimal){
        (self.ltv, self.liquidation_threshold)
    }

    pub fn get_total_supply_borrow(&self) -> (Decimal, Decimal){
        (
            self.get_total_normalized_supply(),
            self.get_total_normalized_borrow()
        )
    }


    fn get_total_stable_borrow(&self) -> Decimal{
        // last_update_epoch, 是每次计息的时刻，每次计息时：存款，借款（稳定，可变)同时计算，但时稳定利率并不
        let delta_epoch = Runtime::current_epoch().number() - self.last_update_epoch;
        if delta_epoch > 0u64 {
            let delta_epoch_year = Decimal::from(delta_epoch) / Decimal::from(EPOCH_OF_YEAR);
            return self.stable_borrow_amount * (Decimal::ONE + delta_epoch_year) * self.stable_avg_rate;
        }
        self.stable_borrow_amount
    }

    /// 稳定币的利率只需在发生变化（借还）时变更和保存，须同时更新index. 调用此方法之前已经更新AssetState相关字段。
    fn update_interest_rate(&mut self){
        let (borrow_interest_rate, _, supply_interest_rate) = self.get_interest_rates(Decimal::ZERO, false);
        self.borrow_interest_rate = borrow_interest_rate;
        self.supply_interest_rate = supply_interest_rate;
    }

    fn get_total_normalized_supply(&self) -> Decimal{
        let token_res_mgr: ResourceManager = ResourceManager::from_address(self.token);
        token_res_mgr.total_supply().unwrap()
    }

    fn get_total_normalized_borrow(&self) -> Decimal{
        self.normalized_total_borrow
    }

    fn calc_interest_rate(&self, variable_borrow: Decimal, stable_borrow: Decimal, supply: Decimal) -> (Decimal, Decimal, Decimal){
        debug!("calc_interest_rate.0, var:{}, stable:{}, supply:{}", variable_borrow, stable_borrow, supply);
        let total_debt = variable_borrow + stable_borrow;
        let borrow_ratio = if supply == Decimal::ZERO { Decimal::ZERO} else {total_debt / supply};
        debug!("calc_interest_rate.1, borrow_ratio:{}, ", borrow_ratio);
        let variable_borrow_rate = self.def_interest_model.get_borrow_interest_rate(borrow_ratio, self.interest_model.clone());
        let stable_borrow_rate = self.def_interest_model.get_stable_interest_rate(borrow_ratio, self.interest_model.clone());
        debug!("calc_interest_rate.2, var_ratio:{}, stable_ratio:{} ", variable_borrow_rate, stable_borrow_rate);
        let overall_borrow_rate = if total_debt == Decimal::ZERO { Decimal::ZERO } else {(variable_borrow * variable_borrow_rate + stable_borrow * stable_borrow_rate)/total_debt};

        let interest = total_debt * overall_borrow_rate * (Decimal::ONE - self.insurance_ratio);
        let supply_rate = if supply == Decimal::ZERO { Decimal::ZERO} else {interest / supply};
        debug!("calc_interest_rate.3, interest:{}, overall_borrow_rate:{}, supply_rate:{} ", interest, overall_borrow_rate, supply_rate);
        (variable_borrow_rate, stable_borrow_rate, supply_rate)
    }

    fn get_total_supply_with_index(&self, current_supply_index: Decimal) -> Decimal{
        self.get_total_normalized_supply() * current_supply_index
    }

    fn get_total_borrow_with_index(&self, current_borrow_index: Decimal) -> Decimal{
        self.normalized_total_borrow * current_borrow_index
    }
}

#[blueprint]
mod dexian_lending {
    
    enable_method_auth!{
        roles{
            admin => updatable_by: [];
        },
        methods {
            new_pool => restrict_to: [admin, OWNER];
            // withdraw_fee => restrict_to: [admin, OWNER];  // withdraw_fee should restrict to Pool?

            // readonly
            get_asset_price => PUBLIC;
            get_current_index => PUBLIC;
            get_interest_rate => PUBLIC;
            get_current_supply_borrow => PUBLIC;
            get_total_supply_borrow => PUBLIC;
            get_avaliable => PUBLIC;
            get_ltv_and_liquidation_threshold => PUBLIC;
            get_last_update_epoch => PUBLIC;
            get_cdp_digest => PUBLIC;

            //business method
            supply => PUBLIC;
            withdraw => PUBLIC;
            borrow_variable => PUBLIC;
            borrow_stable => PUBLIC;
            repay => PUBLIC;
            liquidation => PUBLIC;

        }
    }
    
    struct LendingFactory {
        /// Price Oracle
        price_oracle: Global<PriceOracle>,
        //Status of each asset in the lending pool, I.E.: XRD --> AssetState(XRD)
        states: HashMap<ResourceAddress, AssetState>,
        // address map for supply token(K) and deposit token(V), I.E. dxXRD --> XRD
        deposit_asset_map: HashMap<ResourceAddress, ResourceAddress>,
        // vault for each collateral asset(supply token), I.E. dxXRD --> Vault(dxXRD)
        collateral_vaults: HashMap<ResourceAddress, Vault>,
        // Cash of each asset in the lending pool, I.E. XRD --> Vault(XRD)
        vaults: HashMap<ResourceAddress, Vault>,
        // CDP token define
        cdp_res_addr: ResourceAddress,
        // CDP id counter
        cdp_id_counter: u64,
        // lending pool admin badge.
        admin_badge: ResourceAddress,
        // minter
        minter: Vault,

    }

    impl LendingFactory {
        
        pub fn instantiate_lending_factory(price_oracle: Global<PriceOracle>) -> (Global<LendingFactory>, Bucket) {
            
            let admin_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                //set divisibility to none to ensure that the admin badge can not be fractionalized.
                .divisibility(DIVISIBILITY_NONE)
                .mint_initial_supply(Decimal::ONE);
            
            let minter = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .mint_initial_supply(Decimal::ONE);
            
            let minter_addr = minter.resource_address();
            let cdp_res_addr = ResourceBuilder::new_integer_non_fungible::<CollateralDebtPosition>(OwnerRole::None)
                .metadata(metadata!(init{
                    "symbol" => "CDP", locked;
                    "name" => "DeXian CDP Token", locked;
                }))
                .mint_roles(mint_roles!( 
                    minter => rule!(require(minter_addr));
                    minter_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(minter_addr));
                    burner_updater => rule!(deny_all);
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(minter_addr));
                    non_fungible_data_updater_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply().address();
            
            // let price_oracle_component = PriceOraclePackageTarget::at(airdrop_package_address, "PriceOracle").new();
            // let price_oracle: PriceOracleComponentTarget = price_oracle_address.into();

            // let rules = AccessRulesConfig::new()
            //     .method("new_pool", rule!(require(admin_badge.resource_address())),  AccessRule::DenyAll)
            //     .method("withdraw_fee", rule!(require(admin_badge.resource_address())), AccessRule::DenyAll)
            //     .default(AccessRule::AllowAll, AccessRule::DenyAll);

            // Instantiate a LendingFactory component
            let component = Self {
                states: HashMap::new(),
                deposit_asset_map: HashMap::new(),
                collateral_vaults: HashMap::new(),
                vaults: HashMap::new(),
                cdp_id_counter: 0u64,
                minter: Vault::with_bucket(minter.into()),
                admin_badge: admin_badge.resource_address(),
                cdp_res_addr,
                price_oracle
            }
            .instantiate()
            .prepare_to_globalize(
                OwnerRole::Fixed(rule!(require(admin_badge.resource_address())))   // owner & admin 应该分开，互相独立？
            ).roles(
                roles!(
                    admin => rule!(require(admin_badge.resource_address()));
                )
            ).globalize();
            // let component = component.globalize_with_access_rules(rules);
            
            (component, admin_badge.into())
        }

        fn ceil(dec: Decimal) -> Decimal{
            dec.checked_round(18, RoundingMode::ToPositiveInfinity).unwrap()
        }

        fn floor(dec: Decimal) -> Decimal{
            dec.checked_round(18, RoundingMode::ToNegativeInfinity).unwrap()
        }


        pub fn new_pool(&mut self, asset_address: ResourceAddress, 
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal, interest_model: InterestModel, def_interest_model: Global<DefInterestModel>) -> ResourceAddress  {
            let res_mgr = ResourceManager::from_address(asset_address);
            // let symbol: String = ResourceManager::from_address(resource_address).get_metadata::<&str, String>("symbol").unwrap().into();

            let origin_symbol: String = res_mgr.get_metadata::<&str, String>("symbol").unwrap().unwrap();
            let dx_token = ResourceBuilder::new_fungible(OwnerRole::None)
                .metadata(metadata!(init{
                    "symbol" => format!("dx{}", origin_symbol), locked;
                    "name" => format!("DeXian Lending LP token({}) ", origin_symbol), locked;
                }))
                .mint_roles(mint_roles!( 
                    minter => rule!(require(self.minter.resource_address()));
                    minter_updater => rule!(deny_all);
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(self.minter.resource_address()));
                    burner_updater => rule!(deny_all);
                ))
                .create_with_no_initial_supply().address();

            let current_epoch = Runtime::current_epoch().number();
            let asset_state = AssetState{
                supply_index: Decimal::ONE,
                borrow_index: Decimal::ONE,
                borrow_interest_rate: Decimal::ZERO,
                supply_interest_rate: Decimal::ZERO,
                insurance_balance: Decimal::ZERO,
                token: dx_token,
                normalized_total_borrow: Decimal::ZERO,
                stable_borrow_amount: Decimal::ZERO,
                stable_avg_rate: Decimal::ZERO,
                last_update_epoch: current_epoch,
                stable_borrow_last_update: current_epoch,
                ltv,
                liquidation_threshold,
                liquidation_bonus,
                insurance_ratio,
                interest_model,
                def_interest_model
            };

            self.states.insert(asset_address, asset_state);
            self.deposit_asset_map.insert(dx_token, asset_address);
            self.vaults.insert(asset_address, Vault::new(asset_address));
            dx_token
        }

        fn get_max_loan_amount(&self, deposit_asset: ResourceAddress, deposit_amount: Decimal, ltv: Decimal, borrow_asset: ResourceAddress) -> Decimal{
            deposit_amount * self.get_asset_price(deposit_asset) * ltv / self.get_asset_price(borrow_asset)
        }

        pub fn get_asset_price(&self, asset_addr: ResourceAddress) -> Decimal{
            // let component: &Component = resource_manager!(self.oracle_addr);
            // component.call::<Decimal>("get_price_quote_in_xrd", args![asset_addr])
            self.price_oracle.get_price_quote_in_xrd(asset_addr)
        }

        pub fn get_current_index(&self, asset_addr: ResourceAddress) -> (Decimal, Decimal){
            assert!(self.states.contains_key(&asset_addr), "unknown asset!");
            self.states.get(&asset_addr).unwrap().get_current_index()
        }

        pub fn get_interest_rate(&self, asset_addr: ResourceAddress) -> (Decimal, Decimal, Decimal){
            assert!(self.states.contains_key(&asset_addr), "unknown asset!");
            self.states.get(&asset_addr).unwrap().get_interest_rates(Decimal::ZERO, false)
        }

        pub fn get_current_supply_borrow(&self, asset_addr: ResourceAddress) -> (Decimal, Decimal){
            assert!(self.states.contains_key(&asset_addr), "unknown asset!");
            self.states.get(&asset_addr).unwrap().get_current_supply_borrow()
        }

        pub fn get_avaliable(&self, asset_addr: ResourceAddress) -> Decimal{
            assert!(self.vaults.contains_key(&asset_addr), "unknown asset addresss");
            self.vaults.get(&asset_addr).unwrap().amount()
        }

        pub fn get_ltv_and_liquidation_threshold(&self, asset_addr:ResourceAddress) -> (Decimal, Decimal){
            assert!(self.vaults.contains_key(&asset_addr), "unknown asset addresss");
            self.states.get(&asset_addr).unwrap().get_ltv_and_liquidation_threshold()
        }
    
        pub fn get_total_supply_borrow(&self, asset_addr:ResourceAddress) -> (Decimal, Decimal){
            assert!(self.vaults.contains_key(&asset_addr), "unknown asset addresss");
            self.states.get(&asset_addr).unwrap().get_total_supply_borrow()
        }
    
        pub fn get_last_update_epoch(&self, asset_addr:ResourceAddress) ->u64{
            assert!(self.vaults.contains_key(&asset_addr), "unknown asset addresss");
            self.states.get(&asset_addr).unwrap().last_update_epoch
        }

        pub fn supply(&mut self, deposit_asset: Bucket) -> Bucket {
            let asset_address = deposit_asset.resource_address();
            assert!(self.states.contains_key(&asset_address) && self.vaults.contains_key(&asset_address), "There is no pool of funds corresponding to the assets!");
            let asset_state = self.states.get_mut(&asset_address).unwrap();
            
            debug!("before update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);
            asset_state.update_index();
            debug!("after update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);

            let amount = deposit_asset.amount();
            let vault = self.vaults.get_mut(&asset_address).unwrap();
            vault.put(deposit_asset);

            let normalized_amount = LendingFactory::floor(amount / asset_state.supply_index);
            
            let dx_token_bucket = self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let _supply_res_mgr = ResourceManager::from_address(asset_state.token);    
                _supply_res_mgr.mint(normalized_amount)
            });

            asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", asset_address.to_hex(), asset_state.get_total_normalized_supply(), asset_state.normalized_total_borrow, asset_state.borrow_interest_rate, asset_state.supply_interest_rate);

            dx_token_bucket
        }


        pub fn withdraw(&mut self, dx_bucket: Bucket) -> Bucket {
            let dx_address = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_address), "unsupported the token!");
            let amount = dx_bucket.amount();
            let asset_address = self.deposit_asset_map.get(&dx_address).unwrap();
            let asset_state = self.states.get_mut(&asset_address).unwrap();

            debug!("before update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);
            asset_state.update_index();
            debug!("after update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);

            let normalized_amount = LendingFactory::floor(amount * asset_state.supply_index);
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let _supply_res_mgr: ResourceManager = ResourceManager::from_address(asset_state.token);
                _supply_res_mgr.burn(dx_bucket);
            });
            let vault = self.vaults.get_mut(&asset_address).unwrap();
            let asset_bucket = vault.take(normalized_amount);
            asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", asset_address.to_hex(), asset_state.get_total_normalized_supply(), asset_state.normalized_total_borrow, asset_state.borrow_interest_rate, asset_state.supply_interest_rate);
            asset_bucket
        }

        pub fn borrow_stable(&mut self, dx_bucket: Bucket, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            let dx_address = dx_bucket.resource_address();
            let deposit_amount = self._update_collateral_and_validate(&dx_bucket, borrow_token, amount);
            self._put_dx_bucket(dx_bucket);
            let (borrow_bucket, cdp_stable_rate) = self._take_stable_borrow(borrow_token, amount);
            let cdp = self._new_cdp(dx_address, borrow_token, amount, deposit_amount, Decimal::ZERO, cdp_stable_rate, true);
            (borrow_bucket, cdp)
        }

        fn _take_stable_borrow(&mut self, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Decimal){
            let borrow_asset_state = self.states.get_mut(&borrow_token).unwrap();
            borrow_asset_state.update_index();
            
            let (_, stable_rate, _) = borrow_asset_state.get_interest_rates(amount, true);
            let weight_debt = borrow_asset_state.stable_borrow_amount * borrow_asset_state.stable_avg_rate + amount * stable_rate;
            borrow_asset_state.stable_borrow_amount += amount;
            borrow_asset_state.stable_avg_rate = weight_debt / (borrow_asset_state.stable_borrow_amount);
            borrow_asset_state.stable_borrow_last_update = Runtime::current_epoch().number();
            
            borrow_asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_asset_state.get_total_normalized_supply(), borrow_asset_state.normalized_total_borrow, borrow_asset_state.borrow_interest_rate, borrow_asset_state.supply_interest_rate);

            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            (borrow_vault.take(amount), stable_rate)
        }

        fn _update_collateral_and_validate(&mut self, dx_bucket: &Bucket, borrow_token: ResourceAddress, amount: Decimal) -> Decimal {
            assert!(self.states.contains_key(&borrow_token), "unsupported the borrow token!");
            let dx_address = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_address), "unsupported the collateral token!");
            let collateral_addr = self.deposit_asset_map.get(&dx_address).unwrap();

            debug!("borrow dx_bucket {}, collateral_addr {}, ", dx_address.to_hex(), collateral_addr.to_hex());
            let collateral_state = self.states.get_mut(&collateral_addr).unwrap();
            assert!(collateral_state.ltv > Decimal::ZERO, "Then token is not colleteral asset!");
            
            collateral_state.update_index();
            
            let supply_index = collateral_state.supply_index;
            let ltv = collateral_state.ltv;
            let supply_amount = dx_bucket.amount();

            let deposit_amount = LendingFactory::floor(supply_amount * supply_index);
            let max_loan_amount = self.get_max_loan_amount(collateral_addr.clone(), deposit_amount, ltv, borrow_token);
            debug!("max loan amount {}, supply_amount:{} deposit_amount:{}, amount:{}", max_loan_amount, supply_amount, deposit_amount, amount);
            assert!(amount < max_loan_amount, "the vault insufficient balance.");
            
            deposit_amount
        }

        fn _put_dx_bucket(&mut self, dx_bucket: Bucket) {
            let dx_address = dx_bucket.resource_address();
            if self.collateral_vaults.contains_key(&dx_address){
                let collateral_vault = self.collateral_vaults.get_mut(&dx_address).unwrap();
                collateral_vault.put(dx_bucket);
            }
            else{
                let vault = Vault::with_bucket(dx_bucket);
                self.collateral_vaults.insert(dx_address, vault);
            }
        }

        fn _take_borrow_bucket(&mut self, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Decimal){
            let borrow_state = self.states.get_mut(&borrow_token).unwrap();
            debug!("before update_index, asset_address{}, normalized:{}, indexes:{},{}", borrow_token.to_hex(), borrow_state.normalized_total_borrow, borrow_state.borrow_index, borrow_state.supply_index);
            borrow_state.update_index();
            debug!("after update_index, asset_address{}, normalized:{}, indexes:{},{}", borrow_token.to_hex(), borrow_state.normalized_total_borrow, borrow_state.borrow_index, borrow_state.supply_index);

            let borrow_normalized_amount = LendingFactory::ceil(amount / borrow_state.borrow_index);
            borrow_state.normalized_total_borrow += borrow_normalized_amount;
            debug!("before update_interest_rate {}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_state.get_total_normalized_supply(), borrow_state.normalized_total_borrow, borrow_state.borrow_interest_rate, borrow_state.supply_interest_rate);
            borrow_state.update_interest_rate();
            debug!("after update_interest_rate {}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_state.get_total_normalized_supply(), borrow_state.normalized_total_borrow, borrow_state.borrow_interest_rate, borrow_state.supply_interest_rate);

            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            (borrow_vault.take(amount), borrow_normalized_amount)
        }

        fn _renew_cdp(&mut self, dx_address: ResourceAddress, borrow_token: ResourceAddress, amount: Decimal, collateral_amount: Decimal, borrow_normalized_amount: Decimal, cdp: Bucket) {

            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let mut data = cdp.resource_manager().get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            assert!(data.borrow_token == borrow_token && data.collateral_token == dx_address , "collateral token and borrow must matches the exists CDP!");
            data.total_borrow += amount;
            data.borrow_amount += amount;
            data.normalized_borrow += borrow_normalized_amount;
            if collateral_amount > Decimal::ZERO{
                data.collateral_amount += collateral_amount;
            }
            data.last_update_epoch =  Runtime::current_epoch().number();
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let cdp_res_mgr: ResourceManager = ResourceManager::from_address(self.cdp_res_addr);
                cdp_res_mgr.update_non_fungible_data(&cdp_id, "total_borrow", data.total_borrow);
                cdp_res_mgr.update_non_fungible_data(&cdp_id, "borrow_amount", data.borrow_amount);
                cdp_res_mgr.update_non_fungible_data(&cdp_id, "normalized_borrow", data.normalized_borrow);
                if collateral_amount > Decimal::ZERO {
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", data.collateral_amount);
                }
                cdp_res_mgr.update_non_fungible_data(&cdp_id, "last_update_epoch", data.total_repay);
            });
        }

        fn _new_cdp(&mut self, dx_address: ResourceAddress, borrow_token: ResourceAddress, amount: Decimal, collateral_amount: Decimal, 
            borrow_normalized_amount: Decimal, cdp_avg_rate:Decimal, is_stable: bool) -> Bucket {
            let data = CollateralDebtPosition{
                collateral_token: dx_address.clone(),
                total_borrow: amount,
                total_repay: Decimal::ZERO,
                normalized_borrow: borrow_normalized_amount,
                collateral_amount: collateral_amount,
                borrow_amount: amount,
                last_update_epoch: Runtime::current_epoch().number(),
                stable_rate: cdp_avg_rate,
                is_stable: is_stable,
                borrow_token
            };
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self.cdp_id_counter += 1;
                let _cdp_res_mgr: ResourceManager = ResourceManager::from_address(self.cdp_res_addr);
                _cdp_res_mgr.mint_non_fungible(&NonFungibleLocalId::integer(self.cdp_id_counter), data)
            })
        }

        pub fn borrow_variable(&mut self, dx_bucket: Bucket, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            let dx_address = dx_bucket.resource_address();
            let deposit_amount = self._update_collateral_and_validate(&dx_bucket, borrow_token, amount);
            self._put_dx_bucket(dx_bucket);
            let (borrow_bucket, normalized_borrow_amount) = self._take_borrow_bucket(borrow_token, amount);
            let cdp = self._new_cdp(dx_address, borrow_token, amount, deposit_amount, normalized_borrow_amount, Decimal::ZERO, false);
            (borrow_bucket, cdp)
        }

        pub fn repay(&mut self, mut repay_token: Bucket, cdp: Bucket) -> (Bucket, Bucket, Option<Bucket>) {
            assert!(
                cdp.resource_address() == self.cdp_res_addr && cdp.amount() == dec!("1"),
                "We can only handle one CDP each time!"
            );

            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let mut cdp_data = cdp.resource_manager().get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            assert!(borrow_token == repay_token.resource_address(), "Must return borrowed coin.");

            let borrow_state = self.states.get_mut(&borrow_token).unwrap();
            debug!("before update_index, borrow normalized:{} total_borrow_normailized:{} indexes:{},{}", cdp_data.normalized_borrow, borrow_state.normalized_total_borrow, borrow_state.supply_index, borrow_state.borrow_index);
            borrow_state.update_index();
            debug!("after update_index, borrow normalized:{} total_borrow_normailized:{} indexes:{},{}", cdp_data.normalized_borrow, borrow_state.normalized_total_borrow, borrow_state.supply_index, borrow_state.borrow_index);
            let borrow_index = borrow_state.borrow_index;
            assert!(borrow_index > Decimal::ZERO, "borrow index error! {}", borrow_index);
            let mut normalized_amount = LendingFactory::floor(repay_token.amount() / borrow_index);
            
            
            let mut repay_amount = repay_token.amount();
            let mut collateral_bucket: Option<Bucket> = None;
            let is_full_repayment = cdp_data.normalized_borrow <= normalized_amount;

            if is_full_repayment {
                // Full repayment repayAmount <= amount
                // because ⌈⌊a/b⌋*b⌉ <= a
                repay_amount = LendingFactory::ceil(cdp_data.normalized_borrow * borrow_index);
                normalized_amount = cdp_data.normalized_borrow;

                let dx_address = cdp_data.collateral_token;
                let collateral_vault = self.collateral_vaults.get_mut(&dx_address).unwrap();
                collateral_bucket = Some(collateral_vault.take(cdp_data.collateral_amount));
            
                cdp_data.collateral_amount = Decimal::ZERO;
            }
    
                // self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                //     cdp.burn();
                // });
                // return (repay_token, collateral_bucket);
            debug!("repay_bucket:{}, normalized_amount:{}, normalized_borrow:{}, repay_amount:{}", repay_amount, normalized_amount, cdp_data.normalized_borrow, repay_amount);
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            borrow_vault.put(repay_token.take(repay_amount));

            cdp_data.total_repay += repay_amount;
            cdp_data.normalized_borrow -= normalized_amount;
            cdp_data.last_update_epoch = Runtime::current_epoch().number();
            borrow_state.normalized_total_borrow -= normalized_amount;

            borrow_state.update_interest_rate();
            
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self.update_after_repay(cdp.resource_address(), &cdp_id, cdp_data, is_full_repayment);
            });

            (repay_token, cdp, collateral_bucket, )
        }

        fn update_after_repay(&self, cdp_res_addr: ResourceAddress, cdp_id: &NonFungibleLocalId, cdp_data: CollateralDebtPosition, is_full_repayment: bool) {
            let res_mgr: ResourceManager = ResourceManager::from_address(cdp_res_addr);
            if is_full_repayment {
                res_mgr.update_non_fungible_data(cdp_id, "collateral_amount", cdp_data.collateral_amount);
            }
            res_mgr.update_non_fungible_data(cdp_id, "total_repay", cdp_data.total_repay);
            res_mgr.update_non_fungible_data(cdp_id, "normalized_borrow", cdp_data.normalized_borrow);
            res_mgr.update_non_fungible_data(cdp_id, "last_update_epoch", cdp_data.last_update_epoch);
        }

        pub fn liquidation(&mut self, mut debt_bucket: Bucket, cdp_id: u64) -> Bucket{
            let (debt, collateral, collateral_in_xrd, debt_in_xrd, collateral_price, _) = self.get_cdp_digest(cdp_id);
            assert!(debt == debt_bucket.resource_address(), "The CDP can not support the repay by the bucket!");
            let collateral_state = self.states.get_mut(&collateral).unwrap();
            assert!(collateral_state.liquidation_threshold >= debt_in_xrd / collateral_in_xrd, "The CDP can not be liquidation yet, the timing too early!");
            collateral_state.update_index();
            let liquidation_bonus = collateral_state.liquidation_bonus;
            let collateral_supply_index = collateral_state.supply_index;

            let debt_state = self.states.get_mut(&debt).unwrap();
            debug!("before update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            debt_state.update_index();
            debug!("after update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            let borrow_index = debt_state.borrow_index;
            assert!(borrow_index > Decimal::ZERO, "borrow index error! {}", borrow_index);
            let mut normalized_amount = LendingFactory::floor(debt_bucket.amount() / borrow_index);

            let mut cdp_data: CollateralDebtPosition = ResourceManager::from_address(self.cdp_res_addr).get_non_fungible_data(&NonFungibleLocalId::integer(cdp_id));
            assert!(cdp_data.normalized_borrow <= normalized_amount,  "Underpayment of value of debt!");
            // repayAmount <= amount
            // because ⌈⌊a/b⌋*b⌉ <= a
            let repay_amount = LendingFactory::ceil(cdp_data.normalized_borrow * borrow_index);
            normalized_amount = cdp_data.normalized_borrow;

            let normalized_collateral = debt_in_xrd / collateral_price * (Decimal::ONE - liquidation_bonus) / collateral_supply_index;
            assert!(cdp_data.collateral_amount > normalized_collateral, "take collateral too many!");
            
            let dx_address = cdp_data.collateral_token;
            let collateral_vault = self.collateral_vaults.get_mut(&dx_address).unwrap();
            let collateral_bucket = collateral_vault.take(normalized_collateral);
            
            cdp_data.collateral_amount -=  normalized_collateral;
            cdp_data.normalized_borrow = Decimal::ZERO;

            debug!("repay_bucket:{}, normalized_amount:{}, normalized_borrow:{}, repay_amount:{}", repay_amount, normalized_amount, cdp_data.normalized_borrow, repay_amount);
            let borrow_vault = self.vaults.get_mut(&debt).unwrap();
            borrow_vault.put(debt_bucket.take(repay_amount));
            debt_state.normalized_total_borrow -= repay_amount;

            debt_state.update_interest_rate();

            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self.update_after_repay(self.cdp_res_addr, &NonFungibleLocalId::integer(cdp_id), cdp_data, true);
            });

            collateral_bucket
        }

        pub fn get_cdp_digest(&self, cdp_id: u64) -> (ResourceAddress, ResourceAddress, Decimal, Decimal, Decimal, Decimal){
            let cdp = ResourceManager::from_address(self.cdp_res_addr).get_non_fungible_data::<CollateralDebtPosition>(&NonFungibleLocalId::integer(cdp_id));
            let borrow_token = cdp.borrow_token;
            let collateral_token = cdp.collateral_token;
            let deposit_asset_addr = self.deposit_asset_map.get(&collateral_token).unwrap();
            let collateral_state = self.states.get(&deposit_asset_addr).unwrap();
            let debt_state = self.states.get(&borrow_token).unwrap();
            

            let deposit_asset_price = self.get_asset_price(deposit_asset_addr.clone());
            let debet_asset_price = self.get_asset_price(borrow_token.clone());
            let (collateral_supply_index, _)= collateral_state.get_current_index();
            let (_, debet_borrow_index) = debt_state.get_current_index();
            

            // return {
            //     "collateral_token": cdp.collateral_token,
            //     "borrow_token": cdp.borrow_token,
            //     "debt_in_xrd": LendingPool::ceil(cdp.normalized_borrow * debet_borrow_index * debet_asset_price),
            //     "collateral_in_xrd": LendingPool::floor(cdp.normalized_collateral * collateral_supply_index * deposit_asset_price)
            //     "debet_asset_price": debet_asset_price,
            //     "collateral_asset_price": deposit_asset_price
            // };
            (
                cdp.borrow_token,
                cdp.collateral_token, 
                LendingFactory::ceil(cdp.normalized_borrow * debet_borrow_index * debet_asset_price),
                LendingFactory::floor(cdp.collateral_amount * collateral_supply_index * deposit_asset_price),
                debet_asset_price,
                deposit_asset_price
            )

        }
    }
}