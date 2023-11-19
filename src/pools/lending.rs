use scrypto::prelude::*;
use crate::utils::*;
use crate::interest::InterestModel;

#[blueprint]
mod lend_pool {

    enable_method_auth!{
        roles{
            admin => updatable_by:[];
            operator => updatable_by: [];
        },
        methods {
            withdraw_insurance => restrict_to: [operator];

            borrow_variable => restrict_to: [operator];
            borrow_stable => restrict_to: [operator];
            repay_stable => restrict_to: [operator];
            repay_variable => restrict_to: [operator];

            // readonly
            get_current_index => PUBLIC;
            get_interest_rate => PUBLIC;
            get_variable_share_quantity => PUBLIC;
            get_deposit_share_quantity => PUBLIC;
            get_stable_interest => PUBLIC;
            get_available => PUBLIC;
            get_last_update => PUBLIC;
            get_redemption_value => PUBLIC;
            get_underlying_value => PUBLIC;

            //business method
            add_liquity => PUBLIC;
            remove_liquity => PUBLIC;
        }
    }
    
    struct LendResourcePool{

        interest_model_cmp: Global<AnyComponent>,
        interest_model: InterestModel,
        
        underlying_token: ResourceAddress,
        deposit_share_token: ResourceAddress,
        
        vault: Vault,
        insurance_balance: Decimal,
        
        deposit_index: Decimal,
        loan_index: Decimal,
        
        last_update: u64,

        insurance_ratio: Decimal,
        
        deposit_interest_rate: Decimal,
        
        variable_loan_interest_rate: Decimal,
        variable_loan_share_quantity: Decimal,
        
        stable_loan_interest_rate: Decimal,
        stable_loan_amount: Decimal,
        stable_loan_last_update: u64
    }


    impl LendResourcePool {

        pub fn instantiate(
            underlying_token: ResourceAddress,
            interest_model_cmp_addr: ComponentAddress,
            interest_model: InterestModel,
            insurance_ratio: Decimal,
            admin_rule: AccessRule,
            pool_mgr_rule: AccessRule
        ) -> (Global<LendResourcePool>, ResourceAddress) {
            let res_mgr = ResourceManager::from_address(underlying_token);
            let origin_symbol: String = res_mgr.get_metadata::<&str, String>("symbol").unwrap().unwrap();

            let (address_reservation, address) = Runtime::allocate_component_address(LendResourcePool::blueprint_id());

            let dx_rule = rule!(require(global_caller(address)));
            let deposit_share_res_mgr = ResourceBuilder::new_fungible(OwnerRole::None)
                .metadata(metadata!(init{
                    "pool" => address, locked;
                    "symbol" => format!("dx{}", origin_symbol), locked;
                    "name" => format!("DeXian Lending LP token({}) ", origin_symbol), locked;
                }))
                .mint_roles(mint_roles! {
                    minter => dx_rule.clone();
                    minter_updater => rule!(deny_all);
                })
                .burn_roles(burn_roles! {
                    burner => dx_rule.clone();
                    burner_updater => rule!(deny_all);
                })
                .create_with_no_initial_supply();

            let deposit_share_addr = deposit_share_res_mgr.address();
            let component = Self {
                interest_model_cmp: Global::from(interest_model_cmp_addr),
                deposit_share_token: deposit_share_addr,
                deposit_index: Decimal::ONE,
                loan_index: Decimal::ONE,
                last_update: 0u64,
                deposit_interest_rate: Decimal::ZERO,
                variable_loan_interest_rate: Decimal::ZERO,
                variable_loan_share_quantity: Decimal::ZERO,
                stable_loan_interest_rate: Decimal::ZERO,
                stable_loan_amount: Decimal::ZERO,
                stable_loan_last_update: 0u64,
                vault: Vault::new(underlying_token),
                insurance_balance: Decimal::ZERO,
                interest_model,
                insurance_ratio,
                underlying_token
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
            
            (component, deposit_share_addr)

        }

        pub fn withdraw_insurance(&mut self, amount: Decimal) -> Bucket{
            assert_amount(amount, self.insurance_balance);
            self.vault.take(amount)
        }

        pub fn get_underlying_value(&self) -> Decimal{
            let res_mgr = ResourceManager::from_address(self.underlying_token);
            let (supply_index, _) = self.get_current_index();
            res_mgr.total_supply().unwrap().checked_mul(supply_index).unwrap()
        }

        pub fn add_liquity(&mut self, bucket: Bucket) -> Bucket{
            assert_resource(&bucket.resource_address(), &self.underlying_token);
            let deposit_amount = bucket.amount();
            let divisibility = get_divisibility(self.deposit_share_token).unwrap();

            self.update_index();
            
            self.vault.put(bucket);
            let mint_amount = floor(deposit_amount.checked_div(self.deposit_index).unwrap(), divisibility);
            let deposit_share_res_mgr = ResourceManager::from_address(self.deposit_share_token);
            let dx_bucket = deposit_share_res_mgr.mint(mint_amount);
            
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            self.update_interest_rate();
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            dx_bucket

        }
        pub fn remove_liquity(&mut self, bucket: Bucket) -> Bucket{
            assert_resource(&bucket.resource_address(), &self.deposit_share_token);

            self.update_index();

            let burn_amount = bucket.amount();
            let divisibility = get_divisibility(self.underlying_token).unwrap();
            let withdraw_amount = floor(self.get_redemption_value(burn_amount), divisibility);
            assert_vault_amount(&self.vault, withdraw_amount);
            let deposit_share_res_mgr = ResourceManager::from_address(self.deposit_share_token);
            deposit_share_res_mgr.burn(bucket);

            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);
            self.update_interest_rate();
            info!("after interest rate:{}, {}, index:{}, {}", self.variable_loan_interest_rate, self.stable_loan_interest_rate, self.deposit_index, self.loan_index);

            self.vault.take(withdraw_amount)

        }

        pub fn borrow_variable(&mut self, borrow_amount: Decimal) -> (Bucket, Decimal){
            assert_vault_amount(&self.vault, borrow_amount);
            
            self.update_index();
            
            let variable_share = borrow_amount.checked_div(self.loan_index).unwrap();
            self.variable_loan_share_quantity = self.variable_loan_share_quantity.checked_add(variable_share).unwrap();
            
            self.update_interest_rate();
            
            (self.vault.take(borrow_amount), variable_share)
        }

        pub fn borrow_stable(&mut self, borrow_amount: Decimal, stable_rate: Decimal) -> Bucket{
            assert_vault_amount(&self.vault, borrow_amount);

            self.update_index();

            self.stable_loan_interest_rate = get_weight_rate(self.stable_loan_amount, self.stable_loan_interest_rate, borrow_amount, stable_rate);
            self.stable_loan_amount = self.stable_loan_amount.checked_add(borrow_amount).unwrap();

            self.update_interest_rate();

            self.vault.take(borrow_amount)

        }


        pub fn repay_variable(&mut self, mut repay_bucket: Bucket, loan_amount: Decimal) -> (Bucket, Decimal){
            assert_resource(&repay_bucket.resource_address(), &self.underlying_token);
            
            self.update_index();

            let divisibility = get_divisibility(self.underlying_token).unwrap();
            let debt_amount = loan_amount.checked_mul(self.loan_index).unwrap();
            let amount = if repay_bucket.amount() > debt_amount {debt_amount} else{repay_bucket.amount()};
            let loan_share = floor(amount.checked_div(self.loan_index).unwrap(), divisibility);

            self.variable_loan_share_quantity = self.variable_loan_share_quantity.checked_sub(loan_share).unwrap();
            self.vault.put(repay_bucket.take(amount));
            
            self.update_interest_rate();
            
            (repay_bucket, loan_share)
        }

        pub fn repay_stable(
            &mut self, 
            mut repay_bucket: Bucket, 
            loan_amount: Decimal,
            rate: Decimal,
            last_epoch_at: u64
        ) -> (Bucket, Decimal, Decimal, Decimal, u64){
            let current_epoch_at = Runtime::current_epoch().number();
            let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
            let delta_epoch = current_epoch_at - last_epoch_at;
            let interest = if delta_epoch <= 0u64 {Decimal::ZERO} else { calc_compound_interest(loan_amount, rate, epoch_of_year, delta_epoch) };
            
            let previous_debt = self.stable_loan_amount.checked_mul(self.stable_loan_interest_rate).unwrap();

            let mut repay_amount = repay_bucket.amount();
            let mut repay_in_borrow = Decimal::ZERO;
            if repay_amount < interest {
                let outstanding_interest = interest.checked_sub(repay_amount).unwrap();
                repay_in_borrow = outstanding_interest.checked_mul(Decimal::from(-1)).unwrap();
                self.stable_loan_amount = self.stable_loan_amount.checked_add(outstanding_interest).unwrap();
                self.stable_loan_interest_rate = previous_debt.checked_add(outstanding_interest.checked_mul(rate).unwrap()).unwrap().checked_div(self.stable_loan_amount).unwrap();
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
                    self.stable_loan_interest_rate = previous_debt.checked_sub(repay_in_borrow.checked_mul(rate).unwrap()).unwrap().checked_div(self.stable_loan_amount).unwrap();
                }
            }
            
            self.vault.put(repay_bucket.take(repay_amount));

            self.update_interest_rate();

            (repay_bucket, repay_amount, repay_in_borrow, interest, current_epoch_at)

        }

        pub fn get_current_index(&self) -> (Decimal, Decimal){
            let delta_epoch = Runtime::current_epoch().number() - self.last_update;
            if delta_epoch <= 0u64{
                return (self.deposit_index, self.loan_index);
            }
            
            let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
            let delta_supply_interest_rate = calc_linear_interest(self.deposit_index, self.deposit_interest_rate, epoch_of_year, delta_epoch);
            let delta_borrow_interest_rate = calc_compound_interest(self.loan_index, self.variable_loan_interest_rate, epoch_of_year, delta_epoch);
            
            (
                self.deposit_index.checked_mul(Decimal::ONE.checked_add(delta_supply_interest_rate).unwrap()).unwrap(),
                self.loan_index.checked_mul(Decimal::ONE.checked_add(delta_borrow_interest_rate).unwrap()).unwrap()
            )
        }

        pub fn get_interest_rate(&self) -> (Decimal, Decimal, Decimal){
            let (supply_index, variable_borrow_index) = self.get_current_index();
            // This supply could be equal to zero.
            let supply: Decimal = self.get_deposit_share_quantity().checked_mul(supply_index).unwrap();
            let variable_borrow = self.get_variable_share_quantity().checked_mul(variable_borrow_index).unwrap();
            let stable_borrow = self.get_stable_loan_value();

            self.calc_interest_rate(supply, variable_borrow, stable_borrow)
        }

        fn calc_interest_rate(&self, supply: Decimal, variable_borrow: Decimal, stable_borrow: Decimal) -> (Decimal, Decimal, Decimal){

            info!("calc_interest_rate.0, var:{}, stable:{}, supply:{}", variable_borrow, stable_borrow, supply);
            let total_debt = variable_borrow.checked_add(stable_borrow).unwrap();
            let borrow_ratio = if supply == Decimal::ZERO { Decimal::ZERO } else { total_debt.checked_div(supply).unwrap() };
            let stable_ratio = if total_debt == Decimal::ZERO {Decimal::ZERO } else { stable_borrow.checked_div(total_debt).unwrap() };
            
            info!("calc_interest_rate.1, borrow_ratio:{}, ", borrow_ratio);
            let (variable_rate, stable_rate) = self.get_interest_rate_from_component(borrow_ratio, stable_ratio);
            info!("calc_interest_rate.2, var_ratio:{}, stable_ratio:{} ", variable_rate, self.stable_loan_interest_rate);
            
            let overall_borrow_rate = if total_debt == Decimal::ZERO { Decimal::ZERO } else {
                variable_borrow.checked_mul(variable_rate).unwrap().checked_add(stable_borrow.checked_mul(self.stable_loan_interest_rate).unwrap()).unwrap()
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
                let (current_supply_index, current_borrow_index) = self.get_current_index();
                
                // get the total equity value
                let variable_borrow: Decimal = self.variable_loan_share_quantity;
                let normalized_supply: Decimal = self.get_deposit_share_quantity();
    
                // interest = equity value * (current index value - [last_update] index value)
                let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
                let recent_variable_interest = variable_borrow.checked_mul(current_borrow_index.checked_sub(self.loan_index).unwrap()).unwrap();
                let recent_stable_interest = calc_compound_interest(self.stable_loan_amount, self.stable_loan_interest_rate, epoch_of_year, delta_epoch);
                let recent_supply_interest = normalized_supply.checked_mul(current_supply_index.checked_sub(self.deposit_index).unwrap()).unwrap();
    
                // the interest rate spread goes into the insurance pool
                self.insurance_balance = self.insurance_balance.checked_add(recent_variable_interest.checked_add(recent_stable_interest).unwrap().checked_sub(recent_supply_interest).unwrap()).unwrap();
    
                info!("update_index({}), borrow_index:{}, current:{}, supply_index:{}, current:{}, stable:{}, stable_avg_rate:{}", Runtime::bech32_encode_address(self.underlying_token), self.loan_index, current_borrow_index, self.deposit_index, current_supply_index, self.stable_loan_amount, self.stable_loan_interest_rate);
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

            let (deposite_rate, variable_rate, _) = self.calc_interest_rate(supply, variable_borrow, stable_borrow);
            self.deposit_interest_rate = deposite_rate;
            self.variable_loan_interest_rate = variable_rate;
        }

        fn get_stable_loan_value(&self) -> Decimal{
            let delta_epoch = Runtime::current_epoch().number() - self.stable_loan_last_update;
            if delta_epoch <= 0u64{
                return self.stable_loan_amount;
            }

            let epoch_of_year = Decimal::from(EPOCH_OF_YEAR);
            let interest = calc_compound_interest(self.stable_loan_amount, self.stable_loan_interest_rate, epoch_of_year, delta_epoch);
            self.stable_loan_amount.checked_add(interest).unwrap()
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

        pub fn get_deposit_share_quantity(&self) -> Decimal{
            let res_mgr = ResourceManager::from_address(self.deposit_share_token);
            res_mgr.total_supply().unwrap()
        }

        /// .
        pub fn get_stable_interest(&self, borrow_amount: Decimal, last_epoch: u64, stable_rate: Decimal) -> Decimal{
            let delta_epoch = Runtime::current_epoch().number() - last_epoch;
            calc_compound_interest(borrow_amount, stable_rate, Decimal::from(EPOCH_OF_YEAR), delta_epoch)
        }

        pub fn get_variable_share_quantity(&self) -> Decimal{
            self.variable_loan_share_quantity
        }

        fn get_interest_rate_from_component(&self, borrow_ratio: Decimal, stable_ratio: Decimal) -> (Decimal, Decimal){
            self.interest_model_cmp.call_raw::<(Decimal, Decimal)>("get_interest_rate", scrypto_args!(borrow_ratio, stable_ratio, self.interest_model.clone()))
        }
    }   

}