
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

    pub fn get_interest_rates(&self, change_amount: Decimal) -> (Decimal, Decimal){
        let (current_supply_index, current_borrow_index) = self.get_current_index();

        // This supply could be equal to zero.
        let supply = self.get_total_supply_with_index(current_supply_index);
        let mut variable_borrow = self.get_total_borrow_with_index(current_borrow_index);
        let stable_borrow = self.get_total_stable_borrow();
        variable_borrow += change_amount;

        self.calc_interest_rate(variable_borrow, stable_borrow, supply)
    }

    pub fn get_stable_interest_rate(&self, change_amount: Decimal) -> Decimal{
        let (current_supply_index, current_borrow_index) = self.get_current_index();
        let supply = self.get_total_supply_with_index(current_supply_index);
        let variable_borrow = self.get_total_borrow_with_index(current_borrow_index);
        let mut stable_borrow = self.get_total_stable_borrow();
        stable_borrow += change_amount;
        self.calc_stable_interest_rate(variable_borrow, stable_borrow, supply)
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
            return self.stable_borrow_amount * (Decimal::ONE + delta_epoch_year * self.stable_avg_rate);
        }
        self.stable_borrow_amount
    }

    /// 稳定币的利率只需在发生变化（借还）时变更和保存，须同时更新index. 调用此方法之前已经更新AssetState相关字段。
    fn update_interest_rate(&mut self){
        let (borrow_interest_rate, supply_interest_rate) = self.get_interest_rates(Decimal::ZERO);
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

    fn calc_stable_interest_rate(&self, variable_borrow: Decimal, stable_borrow: Decimal, supply: Decimal) -> Decimal{
        debug!("calc_stable_interest_rate.0, var:{}, stable:{}, supply:{}", variable_borrow, stable_borrow, supply);
        let total_debt = variable_borrow + stable_borrow;
        let borrow_ratio = if supply == Decimal::ZERO { Decimal::ZERO} else {total_debt / supply};
        let stable_ratio = stable_borrow / total_debt;
        debug!("calc_stable_interest_rate.1, borrow_ratio:{}, stable_ratio:{} ", borrow_ratio, stable_ratio);
        let stable_rate = self.def_interest_model.get_stable_interest_rate(borrow_ratio, stable_ratio, self.interest_model.clone());
        debug!("calc_stable_interest_rate.2, stable_rate:{}", stable_rate);
        stable_rate
    }

    fn calc_interest_rate(&self, variable_borrow: Decimal, stable_borrow: Decimal, supply: Decimal) -> (Decimal, Decimal){
        debug!("calc_interest_rate.0, var:{}, stable:{}, supply:{}", variable_borrow, stable_borrow, supply);
        let total_debt = variable_borrow + stable_borrow;
        let borrow_ratio = if supply == Decimal::ZERO { Decimal::ZERO} else {total_debt / supply};
        debug!("calc_interest_rate.1, borrow_ratio:{}, ", borrow_ratio);
        let variable_borrow_rate = self.def_interest_model.get_borrow_interest_rate(borrow_ratio, self.interest_model.clone());
        debug!("calc_interest_rate.2, var_ratio:{}, stable_ratio:{} ", variable_borrow_rate, self.stable_avg_rate);
        let overall_borrow_rate = if total_debt == Decimal::ZERO { Decimal::ZERO } else {(variable_borrow * variable_borrow_rate + stable_borrow * self.stable_avg_rate)/total_debt};

        let interest = total_debt * overall_borrow_rate * (Decimal::ONE - self.insurance_ratio);
        let supply_rate = if supply == Decimal::ZERO { Decimal::ZERO} else {interest / supply};
        debug!("calc_interest_rate.3, interest:{}, overall_borrow_rate:{}, supply_rate:{} ", interest, overall_borrow_rate, supply_rate);
        (variable_borrow_rate, supply_rate)
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

            //business method
            supply => PUBLIC;
            withdraw => PUBLIC;
            borrow_variable => PUBLIC;
            borrow_stable => PUBLIC;
            extend_borrow => PUBLIC;
            addition_collateral => PUBLIC;
            withdraw_collateral => PUBLIC;
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

        pub fn get_interest_rate(&self, asset_addr: ResourceAddress) -> (Decimal, Decimal){
            assert!(self.states.contains_key(&asset_addr), "unknown asset!");
            self.states.get(&asset_addr).unwrap().get_interest_rates(Decimal::ZERO)
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
            assert!(self.states.contains_key(&asset_address), "There is no pool of funds corresponding to the assets!");
            self._supply(deposit_asset)
        }


        pub fn withdraw(&mut self, dx_bucket: Bucket) -> Bucket {
            let dx_address = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_address), "unsupported the token!");
            let dx_amount = dx_bucket.amount();
            let asset_address = self.deposit_asset_map.get(&dx_address).unwrap();
            let asset_state = self.states.get_mut(&asset_address).unwrap();

            debug!("before update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);
            asset_state.update_index();
            debug!("after update_index, asset_address{} indexes:{},{}", asset_address.to_hex(), asset_state.borrow_index, asset_state.supply_index);

            let amount = LendingFactory::floor(dx_amount * asset_state.supply_index);
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let supply_res_mgr: ResourceManager = ResourceManager::from_address(dx_address);
                supply_res_mgr.burn(dx_bucket);
            });
            let vault = self.vaults.get_mut(&asset_address).unwrap();
            let asset_bucket = vault.take(amount);
            asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", asset_address.to_hex(), asset_state.get_total_normalized_supply(), asset_state.normalized_total_borrow, asset_state.borrow_interest_rate, asset_state.supply_interest_rate);
            asset_bucket
        }

        pub fn withdraw_collateral(&mut self, cdp: Bucket, amount: Decimal) -> (Bucket, Bucket){
            assert!(
                cdp.resource_address() == self.cdp_res_addr && cdp.amount() == Decimal::ONE,
                "Only one CDP can be processed at a time!"
            );
            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let cdp_data = cdp.resource_manager().get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let dx_token = cdp_data.collateral_token;
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();

            let debt_token_price = self.get_asset_price(cdp_data.borrow_token);
            let debt_state = self.states.get(&cdp_data.borrow_token).unwrap();
            let liquidation_threshold = debt_state.liquidation_threshold;
            let (borrow_index, _) = self.get_current_index(cdp_data.borrow_token);
            let debt_in_xrd = cdp_data.normalized_borrow * borrow_index * debt_token_price;

            let underlying_price = self.get_asset_price(underlying_token.clone());
            let underlying_state = self.states.get_mut(underlying_token).unwrap();
            underlying_state.update_index();
            
            let collateral_in_xrd = (cdp_data.collateral_amount * underlying_state.supply_index + amount) * underlying_price;
            assert!(liquidation_threshold > debt_in_xrd / collateral_in_xrd, "Collateral can't be withdrawn in such a way that the CDP is above the liquidation threshold.");

            let normalized_amount = LendingFactory::floor(amount / underlying_state.supply_index);
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let supply_res_mgr: ResourceManager = ResourceManager::from_address(dx_token);
                supply_res_mgr.burn(self.collateral_vaults.get_mut(&dx_token).unwrap().take(normalized_amount));
            });
            underlying_state.update_interest_rate();
            let underlying_bucket = self.vaults.get_mut(&underlying_token).unwrap();
            let withdraw_bucket = underlying_bucket.take(LendingFactory::floor(normalized_amount * underlying_state.supply_index));
            (cdp, withdraw_bucket)
        }

        pub fn borrow_stable(&mut self, dx_bucket: Bucket, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            assert!(self.states.contains_key(&borrow_token), "unsupported the borrow token!");
            let dx_address = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_address), "unsupported the collateral token!");
            let dx_amount = dx_bucket.amount();

            let deposit_amount = self._update_collateral_and_validate(dx_address, dx_amount, borrow_token, amount, Decimal::ZERO);
            self._put_dx_bucket(dx_bucket);
            let (borrow_bucket, cdp_stable_rate) = self._take_stable_borrow(borrow_token, amount);
            let cdp = self._new_cdp(dx_address, borrow_token, amount, deposit_amount, Decimal::ZERO, cdp_stable_rate, true);
            (borrow_bucket, cdp)
        }

        fn _supply(&mut self, deposit_bucket: Bucket) -> Bucket{
            let asset_addr = deposit_bucket.resource_address();
            let asset_state = self.states.get_mut(&asset_addr).unwrap();
            
            debug!("before update_index, asset_address{} indexes:{},{}", asset_addr.to_hex(), asset_state.borrow_index, asset_state.supply_index);
            asset_state.update_index();
            debug!("after update_index, asset_address{} indexes:{},{}", asset_addr.to_hex(), asset_state.borrow_index, asset_state.supply_index);

            let amount = deposit_bucket.amount();
            let vault = self.vaults.get_mut(&asset_addr).unwrap();
            vault.put(deposit_bucket);

            let normalized_amount = LendingFactory::floor(amount / asset_state.supply_index);
            
            let dx_bucket = self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let _supply_res_mgr = ResourceManager::from_address(asset_state.token);    
                _supply_res_mgr.mint(normalized_amount)
            });

            asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", asset_addr.to_hex(), asset_state.get_total_normalized_supply(), asset_state.normalized_total_borrow, asset_state.borrow_interest_rate, asset_state.supply_interest_rate);
            
            dx_bucket
        }

        fn _take_stable_borrow(&mut self, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Decimal){
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            assert!(borrow_vault.amount() > amount, "The borrow vault balance insufficient");
            let borrow_asset_state = self.states.get_mut(&borrow_token).unwrap();
            debug!(
                "before update: {}, supply:{},{}, borrow:{},{}, rate:{},{}, stable:{},{},{}", borrow_token.to_hex(), 
                borrow_asset_state.get_total_normalized_supply(), 
                borrow_asset_state.supply_index,
                borrow_asset_state.normalized_total_borrow, 
                borrow_asset_state.borrow_index,
                borrow_asset_state.borrow_interest_rate, 
                borrow_asset_state.supply_interest_rate,
                borrow_asset_state.stable_borrow_amount,
                borrow_asset_state.stable_avg_rate,
                borrow_asset_state.stable_borrow_last_update
            );
            // 修改stable_avg_rate前，一定要先更新index. 这样已才能将应付利息（变化前的利率）和变化后利率交割清楚。
            borrow_asset_state.update_index();
            debug!(
                "after update: {}, supply:{},{}, borrow:{},{}, rate:{},{}, stable:{},{},{}", borrow_token.to_hex(), 
                borrow_asset_state.get_total_normalized_supply(), 
                borrow_asset_state.supply_index,
                borrow_asset_state.normalized_total_borrow, 
                borrow_asset_state.borrow_index,
                borrow_asset_state.borrow_interest_rate, 
                borrow_asset_state.supply_interest_rate,
                borrow_asset_state.stable_borrow_amount,
                borrow_asset_state.stable_avg_rate,
                borrow_asset_state.stable_borrow_last_update
            );
            
            let stable_rate = borrow_asset_state.get_stable_interest_rate(amount);
            let weight_debt = borrow_asset_state.stable_borrow_amount * borrow_asset_state.stable_avg_rate + amount * stable_rate;
            borrow_asset_state.stable_borrow_amount += amount;
            borrow_asset_state.stable_avg_rate = weight_debt / (borrow_asset_state.stable_borrow_amount);
            borrow_asset_state.stable_borrow_last_update = Runtime::current_epoch().number();
            
            borrow_asset_state.update_interest_rate();
            debug!(
                "_take_stable_borrow: {}, supply:{},{}, borrow:{},{}, rate:{},{}, stable:{},{},{}", borrow_token.to_hex(), 
                borrow_asset_state.get_total_normalized_supply(), 
                borrow_asset_state.supply_index,
                borrow_asset_state.normalized_total_borrow, 
                borrow_asset_state.borrow_index,
                borrow_asset_state.borrow_interest_rate, 
                borrow_asset_state.supply_interest_rate,
                borrow_asset_state.stable_borrow_amount,
                borrow_asset_state.stable_avg_rate,
                borrow_asset_state.stable_borrow_last_update
            );

            (borrow_vault.take(amount), stable_rate)
        }

        fn _update_collateral_and_validate(&mut self, dx_address: ResourceAddress, dx_amount:Decimal, borrow_token: ResourceAddress,
            amount: Decimal, exist_borrow:Decimal) -> Decimal {
            assert!(self.states.contains_key(&borrow_token), "unsupported the borrow token!");
            
            let underlying_address = self.deposit_asset_map.get(&dx_address).unwrap();
            debug!("borrow dx_bucket {}, underlying_address {}, ", dx_address.to_hex(), underlying_address.to_hex());
            let collateral_state = self.states.get_mut(&underlying_address).unwrap();
            assert!(collateral_state.ltv > Decimal::ZERO, "Then token is not colleteral asset!");
            
            debug!("before collateral_state update_index, supply normalized:{} total_borrow_normailized:{} indexes:{},{}", collateral_state.get_total_normalized_supply(), collateral_state.normalized_total_borrow, collateral_state.supply_index, collateral_state.borrow_index);
            collateral_state.update_index();
            debug!("after collateral_state update_index, supply normalized:{} total_borrow_normailized:{} indexes:{},{}", collateral_state.get_total_normalized_supply(), collateral_state.normalized_total_borrow, collateral_state.supply_index, collateral_state.borrow_index);
            
            let supply_index = collateral_state.supply_index;
            let ltv = collateral_state.ltv;

            let deposit_amount = LendingFactory::floor(dx_amount * supply_index);
            let max_loan_amount = self.get_max_loan_amount(underlying_address.clone(), deposit_amount, ltv, borrow_token);
            debug!("max loan amount {}, dx_amount:{} deposit_amount:{}, amount:{}", max_loan_amount, dx_amount, deposit_amount, amount);
            assert!(amount + exist_borrow < max_loan_amount, "exceeds the maximum borrowing amount.");
            
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
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            assert!(borrow_vault.amount() > amount, "The borrow vault balance insufficient");
            
            let borrow_state = self.states.get_mut(&borrow_token).unwrap();
            debug!("before update_index, asset_address{}, normalized:{}, indexes:{},{}", borrow_token.to_hex(), borrow_state.normalized_total_borrow, borrow_state.borrow_index, borrow_state.supply_index);
            borrow_state.update_index();
            debug!("after update_index, asset_address{}, normalized:{}, indexes:{},{}", borrow_token.to_hex(), borrow_state.normalized_total_borrow, borrow_state.borrow_index, borrow_state.supply_index);

            let borrow_normalized_amount = LendingFactory::ceil(amount / borrow_state.borrow_index);
            borrow_state.normalized_total_borrow += borrow_normalized_amount;
            debug!("before update_interest_rate {}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_state.get_total_normalized_supply(), borrow_state.normalized_total_borrow, borrow_state.borrow_interest_rate, borrow_state.supply_interest_rate);
            borrow_state.update_interest_rate();
            debug!("after update_interest_rate {}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_state.get_total_normalized_supply(), borrow_state.normalized_total_borrow, borrow_state.borrow_interest_rate, borrow_state.supply_interest_rate);
            (borrow_vault.take(amount), borrow_normalized_amount)
        }

        fn _update_cdp_data(&mut self, delta_borrow: Decimal, interest: Decimal, delta_collateral: Decimal,
            delta_normalized_borrow: Decimal, cdp_avg_rate:Decimal, cdp_id: NonFungibleLocalId, data: CollateralDebtPosition) {
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                let cdp_res_mgr: ResourceManager = ResourceManager::from_address(self.cdp_res_addr);
                if delta_borrow != Decimal::ZERO || interest != Decimal::ZERO {
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "total_borrow", data.total_borrow + delta_borrow);
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "borrow_amount", data.borrow_amount + delta_borrow + interest);
                }
                if delta_normalized_borrow != Decimal::ZERO {
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "normalized_borrow", data.normalized_borrow + delta_normalized_borrow);
                }
                if delta_collateral != Decimal::ZERO {
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", data.collateral_amount + delta_collateral);
                }
                if cdp_avg_rate != Decimal::ZERO {
                    cdp_res_mgr.update_non_fungible_data(&cdp_id, "stable_rate", cdp_avg_rate);
                }
                cdp_res_mgr.update_non_fungible_data(&cdp_id, "last_update_epoch", Runtime::current_epoch().number());
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
                let cdp_res_mgr: ResourceManager = ResourceManager::from_address(self.cdp_res_addr);
                cdp_res_mgr.mint_non_fungible(&NonFungibleLocalId::integer(self.cdp_id_counter), data)
            })
        }

        pub fn borrow_variable(&mut self, dx_bucket: Bucket, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            let dx_address = dx_bucket.resource_address();
            let dx_amount = dx_bucket.amount();
            let deposit_amount = self._update_collateral_and_validate(dx_address, dx_amount, borrow_token, amount, Decimal::ZERO);
            self._put_dx_bucket(dx_bucket);
            let (borrow_bucket, normalized_borrow_amount) = self._take_borrow_bucket(borrow_token, amount);
            let cdp = self._new_cdp(dx_address, borrow_token, amount, deposit_amount, normalized_borrow_amount, Decimal::ZERO, false);
            (borrow_bucket, cdp)
        }

        pub fn addition_collateral(&mut self, id: u64, bucket: Bucket) {
            let bucket_token = bucket.resource_address();
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp_data = ResourceManager::from_address(self.cdp_res_addr).get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let dx_token = cdp_data.collateral_token;
            let underlying_token = self.deposit_asset_map.get(&dx_token).unwrap();

            assert!(cdp_data.collateral_token == bucket_token || underlying_token == &bucket_token , "The addition of collateralized asset must match the current CDP collateral asset.");
            
            let dx_bucket = if bucket_token == cdp_data.collateral_token { bucket } else { self._supply(bucket) };

            let dx_amount = dx_bucket.amount();
            self._put_dx_bucket(dx_bucket);
            self._update_cdp_data(Decimal::ZERO, Decimal::ZERO, dx_amount, Decimal::ZERO, Decimal::ZERO, cdp_id, cdp_data);

        }

        pub fn extend_borrow(&mut self, cdp: Bucket, amount: Decimal) -> (Bucket, Bucket){
            assert!(
                cdp.resource_address() == self.cdp_res_addr && cdp.amount() == Decimal::ONE,
                "Only one CDP can be processed at a time!"
            );
            let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
            let cdp_data = cdp.resource_manager().get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            let dx_token = cdp_data.collateral_token;
            let dx_amount = cdp_data.collateral_amount;

            let mut cdp_avg_rate = Decimal::ZERO;
            let mut interest = Decimal::ZERO;
            let mut delta_normalized_amount = Decimal::ZERO;
            let borrow_bucket = if cdp_data.is_stable{
                let delta_epoch = Runtime::current_epoch().number() - cdp_data.last_update_epoch;
                let delta_epoch_year = Decimal::from(delta_epoch) / Decimal::from(EPOCH_OF_YEAR);
                interest = LendingFactory::ceil(cdp_data.borrow_amount * delta_epoch_year * cdp_data.stable_rate);
                let exist_borrow = cdp_data.borrow_amount + interest;
                let _deposit_amount = self._update_collateral_and_validate(dx_token, dx_amount, borrow_token, amount, exist_borrow);

                let (borrow_bucket, new_rate) = self._take_stable_borrow(borrow_token, amount);
                let increase_amount = amount + interest;
                cdp_avg_rate = (cdp_data.borrow_amount * cdp_data.stable_rate + increase_amount * new_rate) / (cdp_data.borrow_amount + increase_amount);
                borrow_bucket
            }
            else{
                let (_, current_borrow_index) = self.get_current_index(borrow_token);
                let exist_borrow = cdp_data.normalized_borrow * current_borrow_index;
                let _deposit_amount = self._update_collateral_and_validate(dx_token, dx_amount, borrow_token, amount, exist_borrow);
                let (borrow_bucket, normalized_amount) = self._take_borrow_bucket(borrow_token, amount);
                delta_normalized_amount = normalized_amount;
                borrow_bucket
            };
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self._update_cdp_data(amount, interest, Decimal::ZERO, delta_normalized_amount, cdp_avg_rate, cdp_id, cdp_data);
            });
            
            (borrow_bucket, cdp)
        }

        pub fn repay(&mut self, mut repay_bucket: Bucket, id: u64) -> Bucket {
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp_data = ResourceManager::from_address(self.cdp_res_addr).get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp_data.borrow_token;
            assert!(borrow_token == repay_bucket.resource_address(), "Borrowed asset must be returned.");

            let mut repay_amount = repay_bucket.amount();
            let mut repay_in_borrow = Decimal::ZERO;
            let mut normalized_amount = Decimal::ZERO;
            let borrow_state = self.states.get_mut(&borrow_token).unwrap();
            debug!("before update_index, borrow normalized:{} total_borrow_normailized:{} indexes:{},{}", cdp_data.normalized_borrow, borrow_state.normalized_total_borrow, borrow_state.supply_index, borrow_state.borrow_index);
            borrow_state.update_index();
            debug!("after update_index, borrow normalized:{} total_borrow_normailized:{} indexes:{},{}", cdp_data.normalized_borrow, borrow_state.normalized_total_borrow, borrow_state.supply_index, borrow_state.borrow_index);

            if cdp_data.is_stable{
                let delta_year = LendingFactory::ceil(Decimal::from(Runtime::current_epoch().number() - cdp_data.last_update_epoch) / Decimal::from(EPOCH_OF_YEAR));
                let interest = LendingFactory::ceil(cdp_data.borrow_amount * cdp_data.stable_rate * delta_year);
                let previous_debt = borrow_state.stable_borrow_amount * borrow_state.stable_avg_rate;
                
                if repay_amount < interest {
                    let outstanding_interest = interest - repay_amount;
                    repay_in_borrow = outstanding_interest * Decimal::from(-1);
                    borrow_state.stable_borrow_amount += outstanding_interest;
                    borrow_state.stable_avg_rate = (previous_debt + outstanding_interest * cdp_data.stable_rate) / borrow_state.stable_borrow_amount;
                }
                else{
                    if repay_amount >= cdp_data.borrow_amount + interest {
                        repay_amount = cdp_data.borrow_amount + interest;
                        repay_in_borrow = cdp_data.borrow_amount;
                    }
                    else{
                        repay_in_borrow = repay_amount - interest;
                    }
                    //最后一笔还款可能大于总借款金额。因为每笔贷款还款是单独计算的。
                    if repay_in_borrow >= borrow_state.stable_borrow_amount{
                        borrow_state.stable_borrow_amount = Decimal::ZERO;
                        borrow_state.stable_avg_rate = Decimal::ZERO;
                    }
                    else{
                        borrow_state.stable_borrow_amount -= repay_in_borrow;
                        borrow_state.stable_avg_rate = (previous_debt - repay_in_borrow * cdp_data.stable_rate) / borrow_state.stable_borrow_amount;
                    }
                }
            }
            else{
                let borrow_index = borrow_state.borrow_index;
                normalized_amount = LendingFactory::floor(repay_amount / borrow_index);
                if cdp_data.normalized_borrow <= normalized_amount {
                    // Full repayment repayAmount <= amount
                    // because ⌈⌊a/b⌋*b⌉ <= a
                    repay_amount = LendingFactory::ceil(cdp_data.normalized_borrow * borrow_index);
                    normalized_amount = cdp_data.normalized_borrow;
                }
                debug!("repay_bucket:{}, normalized_amount:{}, normalized_borrow:{}, repay_amount:{}", repay_amount, normalized_amount, cdp_data.normalized_borrow, repay_amount);
                borrow_state.normalized_total_borrow -= normalized_amount;
            }
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            borrow_vault.put(repay_bucket.take(repay_amount));
            
            borrow_state.update_interest_rate();
            
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self._update_after_repay(&cdp_id, cdp_data, repay_amount, repay_in_borrow, normalized_amount, Decimal::ZERO);
            });

            repay_bucket
        }

        fn _update_after_repay(&self, cdp_id: &NonFungibleLocalId, cdp_data: CollateralDebtPosition,
            repay_amount: Decimal, delta_borrow: Decimal, delta_normalized_amount: Decimal, delta_collateral: Decimal
            ) {
            let res_mgr: ResourceManager = ResourceManager::from_address(self.cdp_res_addr);
            res_mgr.update_non_fungible_data(cdp_id, "total_repay", cdp_data.total_repay + repay_amount);
            if delta_normalized_amount != Decimal::ZERO {
                res_mgr.update_non_fungible_data(cdp_id, "normalized_borrow", cdp_data.normalized_borrow - delta_normalized_amount);
            }
            if cdp_data.is_stable && delta_borrow != Decimal::ZERO {
                let new_borrow_amount = cdp_data.borrow_amount - delta_borrow;
                res_mgr.update_non_fungible_data(cdp_id, "borrow_amount", new_borrow_amount);
                if new_borrow_amount == Decimal::ZERO {
                    res_mgr.update_non_fungible_data(cdp_id, "stable_rate", Decimal::ZERO);
                }
            }
            if delta_collateral != Decimal::ZERO{
                res_mgr.update_non_fungible_data(&cdp_id, "collateral_amount", cdp_data.collateral_amount + delta_collateral);
            }
            res_mgr.update_non_fungible_data(cdp_id, "last_update_epoch", Runtime::current_epoch().number());
        }

        pub fn liquidation(&mut self, mut debt_bucket: Bucket, id: u64) -> Bucket{
            let cdp_id = NonFungibleLocalId::integer(id);
            let cdp = ResourceManager::from_address(self.cdp_res_addr).get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
            let borrow_token = cdp.borrow_token;
            assert!(borrow_token == debt_bucket.resource_address(), "The CDP can not support the repay by the bucket!");
            
            let dx_token = cdp.collateral_token;
            let (underlying_asset_price, collateral_supply_index, debt_in_xrd) = self._validate_liquidation(&dx_token, &borrow_token, cdp.normalized_borrow, cdp.collateral_amount);
            let debt_state = self.states.get_mut(&borrow_token).unwrap();
            
            let mut normalized_amount = LendingFactory::floor(debt_bucket.amount() / debt_state.borrow_index);
            assert!(cdp.normalized_borrow <= normalized_amount,  "Underpayment of value of debt!");
            // repayAmount <= amount
            // because ⌈⌊a/b⌋*b⌉ <= a
            let repay_amount = LendingFactory::ceil(cdp.normalized_borrow * debt_state.borrow_index);
            normalized_amount = cdp.normalized_borrow;

            let normalized_collateral = debt_in_xrd / (collateral_supply_index * underlying_asset_price * (Decimal::ONE - debt_state.liquidation_bonus));
            assert!(cdp.collateral_amount > normalized_collateral, "take collateral too many!");
            
            let collateral_vault = self.collateral_vaults.get_mut(&dx_token).unwrap();
            let collateral_bucket = collateral_vault.take(normalized_collateral);
            
            debug!("repay_bucket:{}, normalized_amount:{}, repay_amount:{}", repay_amount, normalized_amount, repay_amount);
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            borrow_vault.put(debt_bucket.take(repay_amount));
            debt_state.normalized_total_borrow -= repay_amount;
            debt_state.update_interest_rate();

            let delta_collateral = collateral_bucket.amount() * Decimal::from(-1);
            self.minter.as_fungible().create_proof_of_amount(Decimal::ONE).authorize(|| {
                self._update_after_repay(&cdp_id, cdp, Decimal::ZERO, Decimal::ZERO, normalized_amount, delta_collateral);
            });

            collateral_bucket
        }

        fn _validate_liquidation(&mut self, dx_token: &ResourceAddress, borrow_token: &ResourceAddress, normalized_borrow: Decimal,
            collateral_amount: Decimal ) -> (Decimal, Decimal, Decimal){
            let underlying_addr = self.deposit_asset_map.get(dx_token).unwrap();
            let debet_asset_price = self.get_asset_price(borrow_token.clone());
            let underlying_asset_price = self.get_asset_price(underlying_addr.clone());

            let collateral_state = self.states.get_mut(underlying_addr).unwrap();
            let (collateral_supply_index, _)= collateral_state.get_current_index();
            
            let collateral_in_xrd = LendingFactory::floor(collateral_amount * collateral_supply_index * underlying_asset_price);
            debug!("before collateral_state update_index, collateral in xrd:{} collateral_amount:{} indexes:{},{}", collateral_in_xrd, collateral_state.get_total_normalized_supply(), collateral_state.supply_index, collateral_state.borrow_index);
            collateral_state.update_index();
            debug!("after collateral_state update_index, collateral in xrd:{} collateral_amount:{} indexes:{},{}", collateral_in_xrd, collateral_state.get_total_normalized_supply(), collateral_state.supply_index, collateral_state.borrow_index);
            // let liquidation_bonus = collateral_state.liquidation_bonus;
            let liquidation_threshold = collateral_state.liquidation_threshold;
            let new_supply_index = collateral_state.supply_index;
            
            let debt_state = self.states.get_mut(borrow_token).unwrap();
            let (_, debet_borrow_index) = debt_state.get_current_index();
            
            let debt_in_xrd = LendingFactory::ceil(normalized_borrow * debet_borrow_index * debet_asset_price);
            
            assert!(liquidation_threshold <= debt_in_xrd / collateral_in_xrd, "The CDP can not be liquidation yet, the timing too early!");

            debug!("before update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            debt_state.update_index();
            debug!("after update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            (underlying_asset_price, new_supply_index, debt_in_xrd)
        }
    }
}