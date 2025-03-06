use scrypto::prelude::*;
use crate::common::list::*;
use crate::common::nft_vaults::NonFungibleVaults;
use crate::validator::keeper::UnstakeData;
use crate::utils::*;
use crate::interest::InterestModel;

#[derive(ScryptoSbor)]
struct FixedEpochBond {
    pub epoch_at: u64,
    pub interest: Decimal,
    pub global_id_list: List<NonFungibleGlobalId>
}

#[blueprint]
#[types(
    ListIndex,
    NonFungibleGlobalId,
    ResourceAddress,
    NonFungibleVault,
    FixedEpochBond
)]
mod lend_pool {

    enable_method_auth!{
        roles{
            admin => updatable_by:[];
            operator => updatable_by: [];
        },
        methods {
            //operator
            withdraw_insurance => restrict_to: [operator];
            borrow_variable => restrict_to: [operator];
            borrow_stable => restrict_to: [operator];
            repay_stable => restrict_to: [operator];
            repay_variable => restrict_to: [operator];
            borrow_fixed_term => restrict_to:[operator];
            repay_fixed_term => restrict_to:[operator];
            add_fixed_term => restrict_to:[operator];
            
            //business method
            add_liquity => PUBLIC;
            remove_liquity => PUBLIC; 

            // readonly
            get_current_index => PUBLIC;
            get_interest_rate => PUBLIC;
            get_variable_share_quantity => PUBLIC;
            get_deposit_share_quantity => PUBLIC;
            get_stable_interest => PUBLIC;
            get_variable_interest => PUBLIC;
            get_available => PUBLIC;
            get_last_update => PUBLIC;
            get_redemption_value => PUBLIC;
            get_underlying_value => PUBLIC;
            get_flashloan_fee_ratio => PUBLIC;
        }
    }
    
    /**
     * LendResourcePool is a pool that allows users to lend their assets to the pool and earn income.
     * The pool will use the assets to provide loans to borrowers and earn interest.
     * The pool will also provide flash loans to users who need to borrow assets for a short period of time.
     * The pool will also fixed-term bonds to users who need to borrow assets for a fixed period of time.
     */
    struct LendResourcePool{

        interest_model_cmp: Global<AnyComponent>,
        interest_model: InterestModel,
        
        underlying_token: ResourceAddress,
        deposit_share_res_mgr: FungibleResourceManager,
        
        vault: FungibleVault,
        insurance_balance: Decimal,
        
        deposit_index: Decimal,
        loan_index: Decimal,
        
        last_update: u64,

        insurance_ratio: Decimal,
        flashloan_fee_ratio: Decimal,
        
        deposit_interest_rate: Decimal,
        
        variable_loan_interest_rate: Decimal,
        variable_loan_share_quantity: Decimal,
        
        stable_loan_interest_rate: Decimal,
        stable_loan_amount: Decimal,
        stable_loan_last_update: u64,

        bond_epochs: Vec<u64>, 
        bonds: KeyValueStore<u64, FixedEpochBond>,
        claim_nfts: NonFungibleVaults
    }


    impl LendResourcePool {

        pub fn instantiate(
            share_divisibility: u8,
            underlying_token: ResourceAddress,
            interest_model_cmp_addr: ComponentAddress,
            interest_model: InterestModel,
            insurance_ratio: Decimal,
            flashloan_fee_ratio: Decimal,
            admin_rule: AccessRule,
            pool_mgr_rule: AccessRule
        ) -> (Global<LendResourcePool>, ResourceAddress) {
            let res_mgr = ResourceManager::from_address(underlying_token);
            let origin_symbol: String = res_mgr.get_metadata::<&str, String>("symbol").unwrap().unwrap();

            let (address_reservation, address) = Runtime::allocate_component_address(LendResourcePool::blueprint_id());

            let dx_rule = rule!(require(global_caller(address)));
            let deposit_share_res_mgr = ResourceBuilder::new_fungible(OwnerRole::None)
                .metadata(metadata!(init{
                    //pool ==> , pool_unit==>
                    "symbol" => format!("dx{}", origin_symbol), locked;
                    "name" => format!("DeXian Lending LP token({}) ", origin_symbol), locked;
                    "underlying" => underlying_token, locked;
                    "icon_url" => "https://dexian.io/images/dx.png", updatable;
                    "info_url" => "https://dexian.io", updatable;
                }))
                .divisibility(share_divisibility)
                .mint_roles(mint_roles! {
                    minter => dx_rule.clone();
                    minter_updater => rule!(deny_all);
                })
                .burn_roles(burn_roles! {
                    burner => dx_rule.clone();
                    burner_updater => rule!(deny_all);
                })
                .create_with_no_initial_supply();

            let component = Self {
                interest_model_cmp: Global::from(interest_model_cmp_addr),
                deposit_index: Decimal::ONE,
                loan_index: Decimal::ONE,
                last_update: 0u64,
                deposit_interest_rate: Decimal::ZERO,
                variable_loan_interest_rate: Decimal::ZERO,
                variable_loan_share_quantity: Decimal::ZERO,
                stable_loan_interest_rate: Decimal::ZERO,
                stable_loan_amount: Decimal::ZERO,
                stable_loan_last_update: 0u64,
                vault: FungibleVault::new(underlying_token),
                insurance_balance: Decimal::ZERO,
                bond_epochs: Vec::new(),
                bonds: KeyValueStore::new(),
                claim_nfts: NonFungibleVaults::new(|| LendResourcePoolKeyValueStore::new_with_registered_type()),
                interest_model,
                insurance_ratio,
                underlying_token,
                flashloan_fee_ratio,
                deposit_share_res_mgr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .roles(
                roles!{
                    admin => admin_rule.clone();
                    operator => pool_mgr_rule.clone();
                }
            )
            .with_address(address_reservation)
            .globalize();
            
            (component, deposit_share_res_mgr.address())

        }

        pub fn withdraw_insurance(&mut self, amount: Decimal) -> FungibleBucket{
            assert_amount(amount, self.insurance_balance);
            self.vault.take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))
        }

        pub fn get_underlying_value(&self) -> Decimal{
            let (supply_index, _) = self.get_current_index();
            self.deposit_share_res_mgr.total_supply().unwrap().checked_mul(supply_index).unwrap()
        }

        pub fn add_liquity(&mut self, bucket: FungibleBucket) -> FungibleBucket{
            assert_resource(&bucket.resource_address(), &self.underlying_token);
            let deposit_amount = bucket.amount();

            self.update_index();
            
            self.vault.put(bucket);
            
            let divisibility = self.deposit_share_res_mgr.resource_type().divisibility().unwrap();
            let mint_amount = floor(deposit_amount.checked_div(self.deposit_index).unwrap(), divisibility);
            let dx_bucket = self.deposit_share_res_mgr.mint(mint_amount);
            
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            self.update_interest_rate();
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            dx_bucket

        }
        pub fn remove_liquity(&mut self, bucket: FungibleBucket) -> FungibleBucket{
            assert_resource(&bucket.resource_address(), &self.deposit_share_res_mgr.address());

            self.update_index();

            let burn_amount = bucket.amount();
            let divisibility = get_divisibility(self.underlying_token).unwrap();
            let withdraw_amount = floor(self.get_redemption_value(burn_amount), divisibility);
            assert!(self.vault.amount() >= withdraw_amount, "the balance in vault is insufficient.");
            self.deposit_share_res_mgr.burn(bucket);

            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            self.update_interest_rate();
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);

            self.vault.take_advanced(withdraw_amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))

        }

        pub fn borrow_variable(&mut self, borrow_amount: Decimal) -> (FungibleBucket, Decimal){
            assert!(self.vault.amount() >= borrow_amount, "the balance in vault is insufficient.");
            
            self.update_index();
            
            let variable_share = ceil(
                borrow_amount.checked_div(self.loan_index).unwrap(), 
                self.deposit_share_res_mgr.resource_type().divisibility().unwrap()
            );
            self.variable_loan_share_quantity = self.variable_loan_share_quantity.checked_add(variable_share).unwrap();
            
            self.update_interest_rate();
            
            (self.vault.take_advanced(borrow_amount, WithdrawStrategy::Rounded(RoundingMode::ToZero)), variable_share)
        }

        pub fn borrow_stable(&mut self, borrow_amount: Decimal, stable_rate: Decimal) -> FungibleBucket{
            assert!(self.vault.amount() >= borrow_amount, "the balance in vault is insufficient.");

            self.update_index();

            self.stable_loan_interest_rate = get_weight_rate(self.stable_loan_amount, self.stable_loan_interest_rate, borrow_amount, stable_rate);
            self.stable_loan_amount = self.stable_loan_amount.checked_add(borrow_amount).unwrap();

            self.update_interest_rate();

            self.vault.take_advanced(borrow_amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))

        }

        pub fn repay_variable(&mut self, mut repay_bucket: FungibleBucket, normalized_amount: Decimal, repay_opt: Option<Decimal>) -> (FungibleBucket, Decimal){
            assert_resource(&repay_bucket.resource_address(), &self.underlying_token);
            
            self.update_index();

            let debt_amount = ceil_by_resource(self.underlying_token, normalized_amount.checked_mul(self.loan_index).unwrap());

            let (actual_amount, normalized) = if repay_bucket.amount() >= debt_amount {
                if repay_opt.is_some_and(|uplimit| uplimit < debt_amount){
                    let amt = repay_opt.unwrap();
                    (amt, floor(amt.checked_div(self.loan_index).unwrap(), self.deposit_share_res_mgr.resource_type().divisibility().unwrap()))
                }
                else{
                    (debt_amount, normalized_amount)
                }
            } else{
                if repay_opt.is_some_and(|uplimit| uplimit < repay_bucket.amount()){
                    let amt = repay_opt.unwrap();
                    (amt, floor(amt.checked_div(self.loan_index).unwrap(), self.deposit_share_res_mgr.resource_type().divisibility().unwrap()))
                }
                else{
                    let amt = repay_bucket.amount();
                    (amt, floor(amt.checked_div(self.loan_index).unwrap(), self.deposit_share_res_mgr.resource_type().divisibility().unwrap()))
                }
            };
            
            self.variable_loan_share_quantity = self.variable_loan_share_quantity.checked_sub(normalized).unwrap();
            self.vault.put(repay_bucket.take(actual_amount));
            
            self.update_interest_rate();
            
            (repay_bucket, normalized)
        }

        pub fn repay_stable(
            &mut self, 
            mut repay_bucket: FungibleBucket, 
            loan_amount: Decimal,
            rate: Decimal,
            last_epoch_at: u64,
            repay_opt: Option<Decimal>
        ) -> (FungibleBucket, Decimal, Decimal, Decimal, u64){
            let current_epoch_at = Runtime::current_epoch().number();
            let delta_epoch = current_epoch_at - last_epoch_at;
            let interest = if delta_epoch <= 0u64 {
                Decimal::ZERO
            } else { 
                ceil_by_resource(
                    self.underlying_token, 
                    calc_compound_interest(
                        loan_amount,
                        rate,
                        Decimal::from(EPOCH_OF_YEAR),
                        delta_epoch
                    ).checked_sub(loan_amount).unwrap()
                )
            };
            
            let previous_debt = self.stable_loan_amount.checked_mul(self.stable_loan_interest_rate).unwrap();

            let mut repay_amount = if repay_opt.is_some_and(|uplimit|uplimit<repay_bucket.amount()){ repay_opt.unwrap() } else { repay_bucket.amount() };
            let repay_in_borrow: Decimal;
            if repay_amount < interest {
                let outstanding_interest = interest.checked_sub(repay_amount).unwrap();
                repay_in_borrow = outstanding_interest.checked_mul(Decimal::from(-1)).unwrap();
                self.stable_loan_amount = self.stable_loan_amount.checked_add(outstanding_interest).unwrap();
                self.stable_loan_interest_rate = previous_debt.checked_add(
                    outstanding_interest.checked_mul(rate).unwrap()
                ).unwrap().checked_div(
                    self.stable_loan_amount
                ).unwrap();
            }
            else{
                let should_paid = loan_amount.checked_add(interest).unwrap();
                if repay_amount >= should_paid {
                    repay_amount = should_paid;
                    repay_in_borrow = loan_amount;
                }
                else{
                    repay_in_borrow = repay_amount.checked_sub(interest).unwrap();
                }
                
                // The final repayment may be greater than the total amount borrowed.
                // This is because each loan repayment is calculated separately.
                if repay_in_borrow >= self.stable_loan_amount{
                    self.stable_loan_amount = Decimal::ZERO;
                    self.stable_loan_interest_rate = Decimal::ZERO;
                }
                else{
                    self.stable_loan_amount = self.stable_loan_amount.checked_sub(repay_in_borrow).unwrap();
                    self.stable_loan_interest_rate = previous_debt.checked_sub(
                        repay_in_borrow.checked_mul(rate).unwrap()
                    ).unwrap().checked_div(
                        self.stable_loan_amount
                    ).unwrap();
                }
            }
            
            self.vault.put(repay_bucket.take(repay_amount));

            self.update_interest_rate();

            (repay_bucket, repay_amount, repay_in_borrow, interest, current_epoch_at)

        }

        pub fn borrow_fixed_term(&mut self, amount: Decimal) -> FungibleBucket {
            assert!(self.vault.amount() >= amount, "Insufficient vault amount!");
            self.vault.take_advanced(amount, WithdrawStrategy::Rounded(RoundingMode::ToZero))
        }

        pub fn repay_fixed_term(&mut self, mut repay_bucket: FungibleBucket, amount: Decimal, fee: Decimal) -> FungibleBucket{
            let total = ceil_by_resource(self.underlying_token.clone(), amount.checked_add(fee).unwrap());
            assert!(repay_bucket.amount() >= total, "Insufficient repay amount!");
            self.vault.put(repay_bucket.take(total));
            if fee > Decimal::ZERO {
                self.update_index();
                
                let (supply_index, _) = self.get_current_index();
                let supply: Decimal = self.get_deposit_share_quantity().checked_mul(supply_index).unwrap();
                
                let insurance = fee.checked_mul(self.insurance_ratio).unwrap();
                self.insurance_balance = self.insurance_balance.checked_add(insurance).unwrap();
                let cumulate_to_supply_index = fee.checked_sub(insurance).unwrap().checked_div(supply).unwrap();
                self.deposit_index = supply_index.checked_add(cumulate_to_supply_index).unwrap();

                self.update_interest_rate();
            }
            repay_bucket
        }

        pub fn add_fixed_term(&mut self, claim_nft: NonFungibleBucket, interest: Decimal){
            let nft_id = claim_nft.non_fungible_global_id();
            let data = claim_nft.non_fungible::<UnstakeData>().data();
            let epoch_at = data.claim_epoch.number();
            
            match self.bond_epochs.binary_search(&epoch_at) {
                Ok(_) => (),
                Err(index) => self.bond_epochs.insert(index, epoch_at),
            }

            if self.bonds.get(&epoch_at).is_none() {
                let mut global_id_list: List<NonFungibleGlobalId> = List::new(||LendResourcePoolKeyValueStore::new_with_registered_type());
                global_id_list.push(nft_id.clone());
                self.bonds.insert(epoch_at, FixedEpochBond{
                    epoch_at,
                    interest,
                    global_id_list
                });
            }
            else{
                let mut entry = self.bonds.get_mut(&epoch_at).unwrap();
                entry.interest = entry.interest.checked_add(interest).unwrap();
                entry.global_id_list.push(nft_id);
            }

            self.claim_nfts.put(claim_nft);
        }

        ///
        /// Retrieves the total amount of mature bonds and the total interest of the mature bonds.
        fn get_mature_bonds(&self) -> (Decimal, Decimal) {
            let current_epoch = Runtime::current_epoch().number();
            let mut sum = Decimal::ZERO;
            let mut interest = Decimal::ZERO;
            
            for epoch in self.bond_epochs.iter() {
                if *epoch > current_epoch {
                    break;
                }
                let epoch_entry = self.bonds.get(epoch);
                if epoch_entry.is_some() {
                    let entry = epoch_entry.unwrap();
                    let nft_ids = entry.global_id_list.range(0, entry.global_id_list.len());
                    sum = sum.checked_add(Self::sum_claim_amount(nft_ids)).unwrap();
                    interest = interest.checked_add(entry.interest).unwrap();
                }
            }
            (sum, interest)
        }

        fn sum_claim_amount(nft_ids: Vec<NonFungibleGlobalId>) -> Decimal{
            let mut sum = Decimal::ZERO;
            for nft_id in nft_ids {
                let data = NonFungibleResourceManager::from(nft_id.resource_address()).get_non_fungible_data::<UnstakeData>(&nft_id.local_id());
                sum = sum.checked_add(data.claim_amount).unwrap();
            }
            sum
        }

        pub fn get_current_index(&self) -> (Decimal, Decimal){
            let current_epoch = Runtime::current_epoch().number();
            let delta_epoch = current_epoch - self.last_update;
            if delta_epoch <= 0u64{
                return (self.deposit_index, self.loan_index);
            }
            
            let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
            // let delta_supply_interest_rate = calc_linear_rate(self.deposit_interest_rate, epoch_of_year, delta_epoch);
            // info!("epoch:{}-{}, delta_epoch:{}, supply:{}==>{}, borrow:{}==>{}", current_epoch, self.last_update, delta_epoch, self.deposit_interest_rate,delta_supply_interest_rate, self.variable_loan_interest_rate, delta_borrow_interest_rate);
            let mut index_of_deposit = calc_linear_interest(self.deposit_index, self.deposit_interest_rate, epoch_of_year, delta_epoch);
            let (_, mature_interest) = self.get_mature_bonds();
            if mature_interest > Decimal::ZERO {
                let deposit_funds = self.get_deposit_share_quantity().checked_mul(index_of_deposit).unwrap();
                let delta_index = mature_interest.checked_mul(
                    Decimal::ONE - self.insurance_ratio
                ).unwrap().checked_div(deposit_funds).unwrap();
                index_of_deposit = index_of_deposit.checked_add(delta_index).unwrap();
            }
            (
                index_of_deposit,
                calc_compound_interest(self.loan_index, self.variable_loan_interest_rate, epoch_of_year, delta_epoch)
            )
        }

        pub fn get_interest_rate(&self, stable_borrow_amount: Decimal) -> (Decimal, Decimal, Decimal){
            let (supply_index, variable_borrow_index) = self.get_current_index();
            // This supply could be equal to zero.
            let supply: Decimal = self.get_deposit_share_quantity().checked_mul(supply_index).unwrap();
            let variable_borrow = self.get_variable_share_quantity().checked_mul(variable_borrow_index).unwrap();
            let stable_borrow = self.get_stable_loan_value().checked_add(stable_borrow_amount).unwrap();

            self.calc_interest_rate(supply, variable_borrow, stable_borrow)
        }

        fn calc_interest_rate(&self, supply: Decimal, variable_borrow: Decimal, stable_borrow: Decimal) -> (Decimal, Decimal, Decimal){

            info!("calc_interest_rate.0, var:{}, stable:{}, supply:{}", variable_borrow, stable_borrow, supply);
            let total_debt = variable_borrow.checked_add(stable_borrow).unwrap();
            let borrow_ratio = if supply == Decimal::ZERO { Decimal::ZERO } else { total_debt.checked_div(supply).unwrap() };
            let stable_ratio = if total_debt == Decimal::ZERO {Decimal::ZERO } else { stable_borrow.checked_div(total_debt).unwrap() };
            
            info!("calc_interest_rate.1, borrow_ratio:{}, stable_ratio:{}", borrow_ratio, stable_ratio);
            let (variable_rate, stable_rate) = self.get_interest_rate_from_component(borrow_ratio, stable_ratio);
            info!("calc_interest_rate.2, variable_rate:{}, stable_rate:{} ", variable_rate, stable_rate);
            
            let overall_borrow_rate = if total_debt == Decimal::ZERO { Decimal::ZERO } else {
                variable_borrow.checked_mul(variable_rate).unwrap().checked_add(stable_borrow.checked_mul(stable_rate).unwrap()).unwrap()
                .checked_div(total_debt).unwrap()
            };
            
            //TODO: supply_rate = overall_borrow_rate * (1-insurance_ratio) * borrow_ratio ?
            let interest = total_debt.checked_mul(overall_borrow_rate).unwrap().checked_mul(Decimal::ONE.checked_sub(self.insurance_ratio).unwrap()).unwrap();
            let supply_rate = if supply == Decimal::ZERO { Decimal::ZERO} else {interest.checked_div(supply).unwrap()};
            info!("calc_interest_rate.3, interest:{}, overall_borrow_rate:{}, supply_rate:{} ", interest, overall_borrow_rate, supply_rate);
        
            (variable_rate, stable_rate, supply_rate)
        }

        fn update_index(&mut self) {
            let current_epoch = Runtime::current_epoch().number();
            let delta_epoch = current_epoch - self.last_update;
            if delta_epoch > 0u64 {
                // Liquidate matured bonds (NFTs) and distribute the accrued returns to all depositors (deposit share holders).
                self.claim_matured_bonds();

                let (current_supply_index, current_borrow_index) = self.get_current_index();
                
                let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
                // variable loan share quantity
                let variable_borrow: Decimal = self.variable_loan_share_quantity;
                // variable loan interest = variable loan share quantity * (current index value - [last_update] index value)
                let recent_variable_interest = variable_borrow.checked_mul(current_borrow_index.checked_sub(self.loan_index).unwrap()).unwrap();
                // stable loan interest
                let recent_stable_interest = calc_compound_interest(self.stable_loan_amount, self.stable_loan_interest_rate, epoch_of_year, delta_epoch).checked_sub(self.stable_loan_amount).unwrap();
                // deposit share quantity
                let normalized_supply: Decimal = self.get_deposit_share_quantity();
                // deposite interest
                let recent_supply_interest = normalized_supply.checked_mul(current_supply_index.checked_sub(self.deposit_index).unwrap()).unwrap();
                
                // the interest rate spread goes into the insurance pool
                // insurance_balance += variable_interest + stable_interest - recent_supply_interest
                self.insurance_balance = self.insurance_balance.checked_add(
                    recent_variable_interest.checked_add(recent_stable_interest).unwrap()
                    .checked_sub(recent_supply_interest).unwrap()
                ).unwrap();
    
                info!("update_index({}), before loan_index:{}, current:{}, before supply_index:{}, current:{}, stable:{}, stable_avg_rate:{}", Runtime::bech32_encode_address(self.underlying_token), self.loan_index, current_borrow_index, self.deposit_index, current_supply_index, self.stable_loan_amount, self.stable_loan_interest_rate);
                self.deposit_index = current_supply_index;
                self.loan_index = current_borrow_index;
                self.last_update = current_epoch;
    
            }
        }

        fn update_interest_rate(&mut self){
            let (supply_index, variable_borrow_index) = self.get_current_index();
            // This supply could be equal to zero.
            let supply: Decimal = self.get_deposit_share_quantity().checked_mul(supply_index).unwrap();
            let variable_borrow = self.get_variable_share_quantity().checked_mul(variable_borrow_index).unwrap();
            let stable_borrow = self.get_stable_loan_value();

            let (variable_rate, _, deposite_rate) = self.calc_interest_rate(supply, variable_borrow, stable_borrow);
            self.deposit_interest_rate = deposite_rate;
            self.variable_loan_interest_rate = variable_rate;
        }

        /// Claims matured bonds (NFTs) and distributes the accrued returns to all depositors.
        fn claim_matured_bonds(&mut self) {
            let current_epoch = Runtime::current_epoch().number();
            let mut interest = Decimal::ZERO;
            // let mut result: Vec<NonFungibleBucket> = Vec::new();

            while let Some(epoch) = self.bond_epochs.first() {
                if *epoch > current_epoch {
                    break;
                }
                if let Some(entry) = self.bonds.get_mut(epoch) {
                    interest = interest.checked_add(entry.interest).unwrap();
                    let nft_ids = entry.global_id_list.range(0, entry.global_id_list.len());
                    if !nft_ids.is_empty() {
                        let nft_buckets = self.claim_nfts.take_nft_batch(nft_ids);
                        for bucket in nft_buckets {
                            let mut validator: Global<Validator> = get_validator(bucket.resource_address());
                            self.vault.put(validator.claim_xrd(bucket));
                        }
                    }
                }
                self.bond_epochs.remove(0);
            }
            
            if interest > Decimal::ZERO {
                let insurance = interest.checked_mul(self.insurance_ratio).unwrap();
                let deposit_funds = self.get_deposit_share_quantity().checked_mul(self.deposit_index).unwrap();
                // It is impossible for `interest` to be positive when `deposit_funds` is zero.
                let delta_index = interest.checked_sub(insurance).unwrap().checked_div(deposit_funds).unwrap();

                self.insurance_balance = self.insurance_balance.checked_add(insurance).unwrap();
                self.deposit_index = self.deposit_index.checked_add(delta_index).unwrap();
            }
        }

        fn get_stable_loan_value(&self) -> Decimal{
            let delta_epoch = Runtime::current_epoch().number() - self.stable_loan_last_update;
            if delta_epoch <= 0u64{
                return self.stable_loan_amount;
            }

            let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
            calc_compound_interest(self.stable_loan_amount, self.stable_loan_interest_rate, epoch_of_year, delta_epoch)
        }

        pub fn get_redemption_value(&self, amount_of_pool_units: Decimal) -> Decimal{
            let (supply_index, _) = self.get_current_index();
            amount_of_pool_units.checked_mul(supply_index).unwrap()
        }
        pub fn get_available(&self) -> Decimal{
            self.vault.amount()
        }

        pub fn get_last_update(&self) -> u64{
            self.last_update
        }

        pub fn get_flashloan_fee_ratio(&self) -> Decimal{
            self.flashloan_fee_ratio
        }

        pub fn get_deposit_share_quantity(&self) -> Decimal{
            self.deposit_share_res_mgr.total_supply().unwrap()
        }

        /// .
        pub fn get_stable_interest(&self, borrow_amount: Decimal, last_epoch: u64, stable_rate: Decimal) -> Decimal{
            let delta_epoch = Runtime::current_epoch().number() - last_epoch;
            calc_compound_interest(borrow_amount, stable_rate, Decimal::from(EPOCH_OF_YEAR), delta_epoch).checked_sub(borrow_amount).unwrap()
        }

        pub fn get_variable_interest(&self, borrow_amount: Decimal) -> Decimal{
            let (_, borrow_index) = self.get_current_index();
            borrow_amount.checked_mul(borrow_index).unwrap()
        }

        pub fn get_variable_share_quantity(&self) -> Decimal{
            self.variable_loan_share_quantity
        }

        fn get_interest_rate_from_component(&self, borrow_ratio: Decimal, stable_ratio: Decimal) -> (Decimal, Decimal){
            self.interest_model_cmp.call_raw::<(Decimal, Decimal)>(
                "get_interest_rate", 
                scrypto_args!(borrow_ratio, stable_ratio, self.interest_model.clone())
            )
        }
    }   

}