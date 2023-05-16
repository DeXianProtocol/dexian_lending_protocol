
use scrypto::prelude::*;

// external_blueprint! {
//     PriceOraclePackageTarget {
//       fn new new(usdt: ResourceAddress, usdt_price: Decimal, usdc: ResourceAddress, usdc_price: Decimal) -> ComponentAddress;
//     }
//   }

external_component! {
    PriceOracleComponentTarget {
        fn get_price_quote_in_xrd(&self, res_addr: ResourceAddress) -> Decimal;
        fn set_price_quote_in_xrd(&mut self, res_addr: ResourceAddress, price_in_xrd: Decimal);
    }
}

external_component! {
    InterestModelComponentTarget {
        fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal;
    }
}

/**
* About 15017 epochs in a year, assuming 35 minute epochs 
* https://learn.radixdlt.com/article/how-long-is-an-epoch-on-radix
* There is no exact timestamp at the moment, so for the time being the period of each epoch (35 minutes) is used for calculation.
**/
const EPOCH_OF_YEAR: u64 = 15017;

#[derive(ScryptoSbor, NonFungibleData)]
pub struct CollateralDebtPosition{
    pub borrow_token: ResourceAddress,
    pub collateral_token: ResourceAddress,
    
    #[mutable]
    pub total_borrow: Decimal,
    #[mutable]
    pub total_repay: Decimal,
    
    #[mutable]
    pub normalized_borrow: Decimal,
    #[mutable]
    pub collateral_amount: Decimal,
    // 这个字段需要去掉。
    #[mutable]
    pub borrow_amount: Decimal,
    #[mutable]
    pub last_update_epoch: u64
}

///asset state
#[derive(ScryptoSbor)]
struct AssetState{
    pub interest_model: InterestModelComponentTarget,
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
    // recipet token of asset
    pub token: ResourceAddress,
    // normalized total borrow.
    pub normalized_total_borrow: Decimal,
    // loan to value
    pub ltv: Decimal,
    // liquidation threshold
    pub liquidation_threshold: Decimal,
    //TODO: flash loan
    // bonus for liquidator
    pub liquidation_bonus: Decimal,
    // last update timestamp
    pub last_update_epoch: u64
}

impl AssetState {
    
    
    // pub fn new(dx_token: ResourceAddress, ltv: Decimal, liquidation_threshold: Decimal, 
    //     liquidation_bonus: Decimal, insurance_ratio: Decimal, interest_model_addr: ComponentAddress) -> AssetStateComponent{
    //         let interest_model: InterestModelComponentTarget = interest_model_addr.into();
    //         Self{
    //             supply_index: Decimal::ONE,
    //             borrow_index: Decimal::ONE,
    //             borrow_interest_rate: Decimal::ZERO,
    //             supply_interest_rate: Decimal::ZERO,
    //             insurance_balance: Decimal::ZERO,
    //             token: dx_token,
    //             normalized_total_borrow: Decimal::ZERO,
    //             last_update_epoch: Runtime::current_epoch(),
    //             ltv,
    //             liquidation_threshold,
    //             liquidation_bonus,
    //             insurance_ratio,
    //             interest_model
    //         }.instantiate()
    //     }

    fn update_index(&mut self) {
        if self.last_update_epoch < Runtime::current_epoch() {
            let (current_supply_index, current_borrow_index) = self.get_current_index();
            
            // get the total equity value
            let normalized_borrow: Decimal = self.normalized_total_borrow;
            let token_res_mgr: ResourceManager = borrow_resource_manager!(self.token);
            let normalized_supply: Decimal = token_res_mgr.total_supply();

            // interest = equity value * (current index value - starting index value)
            let recent_borrow_interest = normalized_borrow * (current_borrow_index - self.borrow_index);
            let recent_supply_interest = normalized_supply * (current_supply_index - self.supply_index);

            // the interest rate spread goes into the insurance pool
            self.insurance_balance += recent_borrow_interest - recent_supply_interest;

            // LOG.info(asset, borrow_index, current_borrow_index, supply_index, current_supply_index);
            self.supply_index = current_supply_index;
            self.borrow_index = current_borrow_index;
            self.last_update_epoch = Runtime::current_epoch();

        }
    }


    pub fn get_current_index(&self) -> (Decimal, Decimal){
        let delta_epoch = Runtime::current_epoch() - self.last_update_epoch;
        let delta_borrow_interest_rate = self.borrow_interest_rate * delta_epoch / EPOCH_OF_YEAR; //AssetState::EPOCH_OF_YEAR;
        let delta_supply_interest_rate = self.supply_interest_rate * delta_epoch / EPOCH_OF_YEAR; //AssetState::EPOCH_OF_YEAR;

        (
            self.supply_index * (Decimal::ONE + delta_supply_interest_rate),
            self.borrow_index * (Decimal::ONE + delta_borrow_interest_rate)
        )

    }

    pub fn get_interest_rates(&self, extra_borrow_amount: Decimal) -> (Decimal, Decimal){
        let (current_supply_index, current_borrow_index) = self.get_current_index();

        let supply = self.get_total_supply_with_index(current_supply_index);
        if supply == Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO);
        }

        let borrow = self.get_total_borrow_with_index(current_borrow_index) + extra_borrow_amount;
        let borrow_ratio = borrow / supply;

        let borrow_interest_rate = self.get_borrow_interest_rate(borrow_ratio);
        
        let borrow_interest = borrow * borrow_interest_rate;
        let supply_interest = borrow_interest * (Decimal::ONE - self.insurance_ratio);

        let supply_interest_rate = supply_interest / supply;
        (borrow_interest_rate, supply_interest_rate)
    }

    pub fn get_current_supply_borrow(&self) -> (Decimal, Decimal){
        let (supply_index, borrow_index) = self.get_current_index();
        (
            self.get_total_supply_with_index(supply_index),
            self.get_total_borrow_with_index(borrow_index)
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

    pub fn get_last_update_epoch(&self) ->u64{
        self.last_update_epoch
    }

    fn update_interest_rate(&mut self) {
        let (borrow_interest_rate, supply_interest_rate) = self.get_interest_rates(Decimal::ZERO);
        self.borrow_interest_rate = borrow_interest_rate;
        self.supply_interest_rate = supply_interest_rate;
    }

    fn get_total_normalized_supply(&self) -> Decimal{
        let token_res_mgr: ResourceManager = borrow_resource_manager!(self.token);
        token_res_mgr.total_supply()
    }

    fn get_total_normalized_borrow(&self) -> Decimal{
        self.normalized_total_borrow
    }

    fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal{
        // let component: &Component = borrow_component!(self.interest_model);
        // component.call::<Decimal>("get_borrow_interest_rate", args![borrow_ratio])
        self.interest_model.get_borrow_interest_rate(borrow_ratio)
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
    struct LendingFactory {
        /// Price Oracle
        price_oracle: PriceOracleComponentTarget,
        //Status of each asset in the lending pool
        states: HashMap<ResourceAddress, AssetState>,
        // address map for supply token(K) and deposit token(V)
        deposit_asset_map: HashMap<ResourceAddress, ResourceAddress>,
        // vault for each collateral asset(supply token)
        collateral_vaults: HashMap<ResourceAddress, Vault>,
        // Cash of each asset in the lending pool
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
        
        pub fn instantiate_lending_factory(price_oracle_address: ComponentAddress) -> (ComponentAddress, Bucket) {
            
            let admin_badge = ResourceBuilder::new_fungible()
                //set divisibility to none to ensure that the admin badge can not be fractionalized.
                .divisibility(DIVISIBILITY_NONE)
                .mint_initial_supply(Decimal::ONE);
            
            let minter = ResourceBuilder::new_fungible()
                .divisibility(DIVISIBILITY_NONE)
                .mint_initial_supply(Decimal::ONE);
            
            let cdp_res_addr = ResourceBuilder::new_integer_non_fungible::<CollateralDebtPosition>()
                .metadata("symbol", "CDP")
                .metadata("name", "DeXian CDP Token")
                .mintable(rule!(require(minter.resource_address())), LOCKED)
                .burnable(rule!(require(minter.resource_address())), LOCKED)
                .updateable_non_fungible_data(rule!(require(minter.resource_address())), LOCKED)
                .create_with_no_initial_supply();
            
            // let price_oracle_component = PriceOraclePackageTarget::at(airdrop_package_address, "PriceOracle").new();
            let price_oracle: PriceOracleComponentTarget = price_oracle_address.into();

            let rules = AccessRulesConfig::new()
                .method("new_pool", rule!(require(admin_badge.resource_address())),  AccessRule::DenyAll)
                .method("withdraw_fee", rule!(require(admin_badge.resource_address())), AccessRule::DenyAll)
                .default(AccessRule::AllowAll, AccessRule::DenyAll);

            // Instantiate a LendingFactory component
            let component = Self {
                states: HashMap::new(),
                deposit_asset_map: HashMap::new(),
                collateral_vaults: HashMap::new(),
                vaults: HashMap::new(),
                cdp_id_counter: 0u64,
                minter: Vault::with_bucket(minter),
                admin_badge: admin_badge.resource_address(),
                cdp_res_addr,
                price_oracle
            }.instantiate();
            let component = component.globalize_with_access_rules(rules);
            
            (component, admin_badge)
        }

        fn ceil(dec: Decimal) -> Decimal{
            dec.round(18u32, RoundingMode::TowardsPositiveInfinity)
        }

        fn floor(dec: Decimal) -> Decimal{
            dec.round(18u32, RoundingMode::TowardsNegativeInfinity)
        }

        pub fn new_pool(&mut self, asset_address: ResourceAddress, 
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal, 
            interest_model_addr: ComponentAddress) -> ResourceAddress  {
            let res_mgr = borrow_resource_manager!(asset_address);

            let origin_symbol = "test";  //res_mgr.metadata().get("symbol").clone();
            let dx_token = ResourceBuilder::new_fungible()
                .metadata("symbol", format!("dx{}", origin_symbol))
                .metadata("name", format!("DeXian Lending LP token({}) ", origin_symbol))
                .mintable(rule!(require(self.minter.resource_address())), LOCKED)
                .burnable(rule!(require(self.minter.resource_address())), LOCKED)
                .create_with_no_initial_supply();
            
            let interest_model = InterestModelComponentTarget::at(interest_model_addr);
            let asset_state = AssetState{
                supply_index: Decimal::ONE,
                borrow_index: Decimal::ONE,
                borrow_interest_rate: Decimal::ZERO,
                supply_interest_rate: Decimal::ZERO,
                insurance_balance: Decimal::ZERO,
                token: dx_token,
                normalized_total_borrow: Decimal::ZERO,
                last_update_epoch: Runtime::current_epoch(),
                ltv,
                liquidation_threshold,
                liquidation_bonus,
                insurance_ratio,
                interest_model
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
            // let component: &Component = borrow_component!(self.oracle_addr);
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
            self.states.get(&asset_addr).unwrap().get_last_update_epoch()
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
            
            let dx_token_bucket = self.minter.authorize(|| {
                let _supply_res_mgr: ResourceManager = borrow_resource_manager!(asset_state.token);    
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
            self.minter.authorize(|| {
                let _supply_res_mgr: ResourceManager = borrow_resource_manager!(asset_state.token);
                _supply_res_mgr.burn(dx_bucket);
            });
            let vault = self.vaults.get_mut(&asset_address).unwrap();
            let asset_bucket = vault.take(normalized_amount);
            asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", asset_address.to_hex(), asset_state.get_total_normalized_supply(), asset_state.normalized_total_borrow, asset_state.borrow_interest_rate, asset_state.supply_interest_rate);
            asset_bucket
        }

        pub fn borrow(&mut self, dx_bucket: Bucket, borrow_token: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            assert!(self.states.contains_key(&borrow_token), "unsupported the borrow token!");
            let dx_address = dx_bucket.resource_address();
            assert!(self.deposit_asset_map.contains_key(&dx_address), "unsupported the collateral token!");
            
            let collateral_addr = self.deposit_asset_map.get(&dx_address).unwrap();
            debug!("borrow dx_bucket {}, collateral_addr {}, ", dx_address.to_hex(), collateral_addr.to_hex());
            let collateral_state = self.states.get_mut(collateral_addr).unwrap();
            assert!(collateral_state.ltv > Decimal::ZERO, "Then token is not colleteral asset!");
            
            collateral_state.update_index();
            
            let supply_index = collateral_state.supply_index;
            let ltv = collateral_state.ltv;
            let supply_amount = dx_bucket.amount();

            let deposit_amount = LendingFactory::floor(supply_amount * supply_index);
            let max_loan_amount = self.get_max_loan_amount(collateral_addr.clone(), deposit_amount, ltv, borrow_token);
            debug!("max loan amount {}, supply_amount:{} deposit_amount:{}, amount:{}", max_loan_amount, supply_amount, deposit_amount, amount);
            assert!(amount < max_loan_amount, "the vault insufficient balance.");

            if self.collateral_vaults.contains_key(&dx_address){
                let collateral_vault = self.collateral_vaults.get_mut(&dx_address).unwrap();
                collateral_vault.put(dx_bucket);
            }
            else{
                let vault = Vault::with_bucket(dx_bucket);
                self.collateral_vaults.insert(dx_address, vault);
            }

            
            let borrow_asset_state = self.states.get_mut(&borrow_token).unwrap();
            borrow_asset_state.update_index();
            
            let borrow_normalized_amount = LendingFactory::ceil(amount / borrow_asset_state.borrow_index);
            borrow_asset_state.normalized_total_borrow += borrow_normalized_amount;
            borrow_asset_state.update_interest_rate();
            debug!("{}, supply:{}, borrow:{}, rate:{},{}", borrow_token.to_hex(), borrow_asset_state.get_total_normalized_supply(), borrow_asset_state.normalized_total_borrow, borrow_asset_state.borrow_interest_rate, borrow_asset_state.supply_interest_rate);

            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            let borrow_bucket = borrow_vault.take(amount);

            let data = CollateralDebtPosition{
                collateral_token: dx_address.clone(),
                total_borrow: amount,
                total_repay: Decimal::ZERO,
                normalized_borrow: borrow_normalized_amount,
                collateral_amount: supply_amount,
                borrow_amount: amount,
                last_update_epoch: Runtime::current_epoch(),
                borrow_token
            };

            let cdp = self.minter.authorize(|| {
                self.cdp_id_counter += 1;
                let _cdp_res_mgr: ResourceManager = borrow_resource_manager!(self.cdp_res_addr);
                _cdp_res_mgr.mint_non_fungible(&NonFungibleLocalId::integer(self.cdp_id_counter), data)
            });
            (borrow_bucket, cdp)
        }


        pub fn repay(&mut self, mut repay_token: Bucket, cdp: Bucket) -> (Bucket, Bucket, Option<Bucket>) {
            assert!(
                cdp.amount() == dec!("1"),
                "We can only handle one CDP each time!"
            );

            let cdp_id = cdp.non_fungible_local_id();
            let mut cdp_data: CollateralDebtPosition = cdp.non_fungible().data();
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
    
                // self.minter.authorize(|| {
                //     cdp.burn();
                // });
                // return (repay_token, collateral_bucket);
            debug!("repay_bucket:{}, normalized_amount:{}, normalized_borrow:{}, repay_amount:{}", repay_amount, normalized_amount, cdp_data.normalized_borrow, repay_amount);
            let borrow_vault = self.vaults.get_mut(&borrow_token).unwrap();
            borrow_vault.put(repay_token.take(repay_amount));

            cdp_data.total_repay += repay_amount;
            cdp_data.normalized_borrow -= normalized_amount;
            cdp_data.last_update_epoch = Runtime::current_epoch();
            borrow_state.normalized_total_borrow -= normalized_amount;

            borrow_state.update_interest_rate();
            
            self.minter.authorize(|| {
                self.update_after_repay(cdp.resource_address(), &cdp_id, cdp_data, is_full_repayment);
            });

            (repay_token, cdp, collateral_bucket, )
        }

        fn update_after_repay(&self, cdp_res_addr: ResourceAddress, cdp_id: &NonFungibleLocalId, cdp_data: CollateralDebtPosition, is_full_repayment: bool) {
            let mut res_mgr: ResourceManager = borrow_resource_manager!(cdp_res_addr);
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
            debt_state.update_index();
            debug!("before update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            debt_state.update_index();
            debug!("after update_index, borrow in xrd:{} total_borrow_normailized:{} indexes:{},{}", debt_in_xrd, debt_state.normalized_total_borrow, debt_state.supply_index, debt_state.borrow_index);
            let borrow_index = debt_state.borrow_index;
            assert!(borrow_index > Decimal::ZERO, "borrow index error! {}", borrow_index);
            let mut normalized_amount = LendingFactory::floor(debt_bucket.amount() / borrow_index);

            let mut cdp_data: CollateralDebtPosition = borrow_resource_manager!(self.cdp_res_addr).get_non_fungible_data(&NonFungibleLocalId::integer(cdp_id));
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

            self.minter.authorize(|| {
                self.update_after_repay(self.cdp_res_addr, &NonFungibleLocalId::integer(cdp_id), cdp_data, true);
            });

            collateral_bucket
        }

        pub fn get_cdp_digest(&self, cdp_id: u64) -> (ResourceAddress, ResourceAddress, Decimal, Decimal, Decimal, Decimal){
            let cdp: CollateralDebtPosition = borrow_resource_manager!(self.cdp_res_addr).get_non_fungible_data(&NonFungibleLocalId::integer(cdp_id));
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