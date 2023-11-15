use scrypto::prelude::*;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
use crate::cdp::cdp_mgr::CollateralDebtManager;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::def_interest_model::DefInterestModel;

#[blueprint]
mod dexian_lending{

    enable_method_auth! {
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [admin];
        },
        methods {
            new_pool => restrict_to: [admin, OWNER];
            withdraw_insurance => restrict_to: [operator, OWNER];

            supply => PUBLIC;
            withdraw => PUBLIC;
            borrow_variable => PUBLIC;
            borrow_stable => PUBLIC;
            extend_borrow => PUBLIC;
            withdraw_collateral => PUBLIC;
            repay => PUBLIC;
            addition_collateral => PUBLIC;
        }
    }
    
    struct LendingProtocol{
        price_oracle: Global<PriceOracle>,
        cdp_mgr: Global<CollateralDebtManager>,
        cdp_res_addr: ResourceAddress,
        admin_badge_res_addr: ResourceAddress,
        op_badge_res_addr: ResourceAddress
    }

    impl LendingProtocol{

        pub fn instantiate(price_signer_pk: String) -> (Global<LendingProtocol>, Global<PriceOracle>, ResourceAddress, Bucket, Bucket){
            let admin_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata!(
                    init {
                        "name" => "Admin Badge".to_owned(), locked;
                        "description" => 
                        "This is a DeXian Lending Protocol admin badge used to authenticate the admin.".to_owned(), locked;
                    }
                ))
                .mint_initial_supply(1);
            let op_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata!(
                    init {
                        "name" => "Operator Badge".to_owned(), locked;
                        "description" => 
                        "This is a DeXian Lending Protocol operator badge used to authenticate the operator.".to_owned(), locked;
                    }
                ))
                .mint_initial_supply(1);

            let (address_reservation, component_address) =
            Runtime::allocate_component_address(LendingProtocol::blueprint_id());

            let cdp_mgr_rule = rule!(require(global_caller(component_address)));

            let price_oracle = PriceOracle::instantiate(
                OwnerRole::Fixed(rule!(require(admin_badge.resource_address()))),
                rule!(require(op_badge.resource_address())), 
                rule!(require(admin_badge.resource_address())),
                price_signer_pk
            );
            let (cdp_mgr, cdp_res_addr) = CollateralDebtManager::instantiate(
                rule!(require(admin_badge.resource_address())), 
                cdp_mgr_rule, 
                price_oracle
            );

            let component = Self{
                admin_badge_res_addr:admin_badge.resource_address(),
                op_badge_res_addr: op_badge.resource_address(),
                price_oracle,
                cdp_mgr,
                cdp_res_addr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge.resource_address()))))
            .with_address(address_reservation)
            .roles(roles! {
                admin => rule!(require(admin_badge.resource_address()));
                operator => rule!(require(op_badge.resource_address()));
            })
            .globalize();
            
            (component, price_oracle, cdp_res_addr, admin_badge.into(), op_badge.into())
        }

        pub fn new_pool(&mut self,
            underlying_token_addr: ResourceAddress,
            interest_model: InterestModel,
            interest_model_cmp_addr: ComponentAddress,
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal
        ) -> ResourceAddress {
            self.cdp_mgr.new_pool(underlying_token_addr, interest_model, interest_model_cmp_addr, ltv, liquidation_threshold, liquidation_bonus, insurance_ratio, rule!(require(self.admin_badge_res_addr)))
        }

        pub fn supply(&mut self, bucket: Bucket) -> Bucket{
            self.cdp_mgr.supply(bucket)
        }

        pub fn withdraw(&mut self, bucket: Bucket) -> Bucket{
            self.cdp_mgr.withdraw(bucket)
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
            self.cdp_mgr.borrow_variable(dx_bucket, borrow_token, borrow_amount, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2)
        }

        pub fn borrow_stable(&mut self,
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
            self.cdp_mgr.borrow_stable(dx_bucket, borrow_token, borrow_amount, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2)
        }

        pub fn extend_borrow(&mut self,
            cdp: Bucket,
            amount: Decimal,
            price1: String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (Bucket, Bucket){
            self.cdp_mgr.extend_borrow(cdp, amount, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2)
        }

        pub fn withdraw_collateral(&mut self,
            cdp: Bucket,
            amount: Decimal,
            price1: String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (Bucket, Bucket){
            self.cdp_mgr.withdraw_collateral(cdp, amount, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2)
        }

        pub fn addition_collateral(&mut self, id: u64, bucket: Bucket){
            self.cdp_mgr.addition_collateral(id, bucket);
        }

        pub fn repay(&mut self, mut repay_bucket: Bucket, id: u64) -> Bucket{
            self.cdp_mgr.repay(repay_bucket, id)
        }

        pub fn withdraw_insurance(&mut self, underlying_token_addr: ResourceAddress, amount: Decimal) -> Bucket{
            self.cdp_mgr.withdraw_insurance(underlying_token_addr, amount)
        }

    }
}