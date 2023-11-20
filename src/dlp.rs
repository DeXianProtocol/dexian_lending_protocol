use scrypto::prelude::*;
use crate::utils::*;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
use crate::cdp::cdp_mgr::CollateralDebtManager;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::def_interest_model::DefInterestModel;


#[blueprint]
#[events(SupplyEvent, WithdrawEvent, CreateCDPEvent, ExtendBorrowEvent, AdditionCollateralEvent, WithdrawCollateralEvent, RepayEvent)]
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
        admin_rule: AccessRule,
        op_rule: AccessRule
    }

    impl LendingProtocol{

        pub fn instantiate(price_signer_pk: String) -> (Global<LendingProtocol>, Global<PriceOracle>, Bucket, Bucket, ResourceAddress){
            
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
            
            let admin_badge_addr = admin_badge.resource_address();
            let op_badge_addr = op_badge.resource_address();
            let (address_reservation, component_address) =
            Runtime::allocate_component_address(LendingProtocol::blueprint_id());

            let cdp_mgr_rule = rule!(require(op_badge_addr) || require(global_caller(component_address)));

            let price_oracle = PriceOracle::instantiate(
                OwnerRole::Fixed(rule!(require(admin_badge_addr))),
                rule!(require(op_badge_addr)),
                rule!(require(admin_badge_addr)),
                price_signer_pk
            );
            let (cdp_mgr, cdp_res_addr) = CollateralDebtManager::instantiate(
                rule!(require(admin_badge_addr)),
                cdp_mgr_rule, 
                price_oracle
            );

            let component = Self{
                admin_rule: rule!(require(admin_badge_addr)),
                op_rule: rule!(require(op_badge_addr)),
                price_oracle,
                cdp_mgr,
                cdp_res_addr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge_addr))))
            .with_address(address_reservation)
            .roles(roles! {
                admin => rule!(require(admin_badge_addr));
                operator => rule!(require(op_badge_addr));
            })
            .globalize();
            
            (component, price_oracle, admin_badge.into(), op_badge.into(), cdp_res_addr)
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
            self.cdp_mgr.new_pool(underlying_token_addr, interest_model, interest_model_cmp_addr, ltv, liquidation_threshold, liquidation_bonus, insurance_ratio, self.admin_rule.clone())
        }

        pub fn supply(&mut self, bucket: Bucket) -> Bucket{
            let supply_token = bucket.resource_address();
            let supply_amount = bucket.amount();
            info!("{} supply {}", Runtime::bech32_encode_address(supply_token), supply_amount);
            let dx_bucket = self.cdp_mgr.supply(bucket);
            let dx_token = dx_bucket.resource_address();
            let dx_amount = dx_bucket.amount();
            info!("{} receipt: {}", Runtime::bech32_encode_address(dx_token), dx_amount);
            Runtime::emit_event(SupplyEvent{supply_token, supply_amount, dx_token, dx_amount});
            dx_bucket
        }

        pub fn withdraw(&mut self, bucket: Bucket) -> Bucket{
            let dx_token = bucket.resource_address();
            let dx_amount = bucket.amount();
            info!("{} burn {}", Runtime::bech32_encode_address(dx_token), dx_amount);
            let withdraw_bucket = self.cdp_mgr.withdraw(bucket);
            let withdraw_token = withdraw_bucket.resource_address();
            let withdraw_amount = withdraw_bucket.amount();
            info!("{} withdraw: {}", Runtime::bech32_encode_address(withdraw_token), withdraw_amount);
            Runtime::emit_event(WithdrawEvent{dx_token, dx_amount, withdraw_token, withdraw_amount});
            withdraw_bucket
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
            let dx_amount = dx_bucket.amount();
            let (collateral_underlying_token,borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.extra_params(dx_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            info!("collateral {}, amount:{}; {} price:{}/{}", Runtime::bech32_encode_address(dx_token), dx_amount, Runtime::bech32_encode_address(collateral_underlying_token), borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            let (borrow_bucket, cdp_bucket) = self.cdp_mgr.borrow_variable(dx_bucket, collateral_underlying_token, borrow_token, borrow_amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            Runtime::emit_event(CreateCDPEvent{dx_token, dx_amount, borrow_token, borrow_amount, cdp_id:cdp_bucket.as_non_fungible().non_fungible_local_id(), is_stable:false});
            (borrow_bucket, cdp_bucket)
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
            let dx_token = dx_bucket.resource_address();
            let dx_amount = dx_bucket.amount();
            let (collateral_underlying_token,borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.extra_params(dx_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            let (borrow_bucket, cdp_bucket) = self.cdp_mgr.borrow_stable(dx_bucket, collateral_underlying_token, borrow_token, borrow_amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            Runtime::emit_event(CreateCDPEvent{dx_token, dx_amount, borrow_token, borrow_amount, cdp_id:cdp_bucket.as_non_fungible().non_fungible_local_id(), is_stable:true});
            (borrow_bucket, cdp_bucket)
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
            let cdp_id: NonFungibleLocalId = cdp.as_non_fungible().non_fungible_local_id();
            let (borrow_token, collateral_underlying_token) = self.cdp_mgr.get_cdp_resource_address(cdp_id.clone());
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.get_price_in_xrd(collateral_underlying_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            info!("collateral {}|{}, {}|{} price:{}/{}", Runtime::bech32_encode_address(collateral_underlying_token), collateral_underlying_token.to_hex(), Runtime::bech32_encode_address(collateral_underlying_token),collateral_underlying_token.to_hex() , borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            let (borrow_bucket, cdp_bucket) = self.cdp_mgr.extend_borrow(cdp, amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            Runtime::emit_event(ExtendBorrowEvent{borrow_token, amount, cdp_id:cdp_id.clone()});
            (borrow_bucket, cdp_bucket)
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
            let cdp_id: NonFungibleLocalId = cdp.as_non_fungible().non_fungible_local_id();
            let (borrow_token, collateral_underlying_token) = self.cdp_mgr.get_cdp_resource_address(cdp_id.clone());
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.get_price_in_xrd(collateral_underlying_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            let (underlying_bucket, cdp_bucket) = self.cdp_mgr.withdraw_collateral(cdp, amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            Runtime::emit_event(WithdrawCollateralEvent{underlying_token:collateral_underlying_token, amount, cdp_id:cdp_id.clone()});
            (underlying_bucket, cdp_bucket)
        }

        pub fn addition_collateral(&mut self, id: u64, bucket: Bucket){
            let cdp_id = NonFungibleLocalId::integer(id);
            let amount = bucket.amount();
            let underlying_token = bucket.resource_address();
            self.cdp_mgr.addition_collateral(id, bucket);
            
            Runtime::emit_event(AdditionCollateralEvent{cdp_id, underlying_token, amount});
        }

        pub fn repay(&mut self, repay_bucket: Bucket, id: u64) -> Bucket{
            let cdp_id: NonFungibleLocalId = NonFungibleLocalId::integer(id);
            let repay_token = repay_bucket.resource_address();
            let bucket_amount = repay_bucket.amount();
            let (bucket, actual_payment) = self.cdp_mgr.repay(repay_bucket, id);
            Runtime::emit_event(RepayEvent{cdp_id, repay_token, bucket_amount, actual_payment});
            bucket
        }

        pub fn withdraw_insurance(&mut self, underlying_token_addr: ResourceAddress, amount: Decimal) -> Bucket{
            self.cdp_mgr.withdraw_insurance(underlying_token_addr, amount)
        }

        fn extra_params(&self,
            dx_token: ResourceAddress,
            borrow_token: ResourceAddress,
            price1: &String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: &String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (ResourceAddress, Decimal, Decimal){
            let collateral_underlying_token = self.cdp_mgr.get_underlying_token(dx_token);
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.get_price_in_xrd(collateral_underlying_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            (collateral_underlying_token, borrow_price_in_xrd, collateral_underlying_price_in_xrd)
        }

        fn get_price_in_xrd(&self,
            collateral_token: ResourceAddress,
            borrow_token: ResourceAddress,
            price1: &String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: &String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (Decimal, Decimal){
            if borrow_token == XRD && collateral_token == quote1 {
                let collateral_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1.clone(), timestamp1, signature1.clone());
                return (Decimal::ONE, collateral_price_in_xrd);
            }
            
            if borrow_token == quote1 && quote2.is_some() && collateral_token == quote2.unwrap(){
                let collateral_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote2.unwrap(), price2.unwrap(), timestamp2.unwrap(), signature2.unwrap());
                let borrow_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1.clone(), timestamp1, signature1.clone());
                return (borrow_price_in_xrd, collateral_price_in_xrd);
            }
            
            if borrow_token == quote1 && collateral_token == XRD {
                let borrow_price_in_xrd = self.price_oracle.get_valid_price_in_xrd(quote1, price1.clone(), timestamp1, signature1.clone());
                return (borrow_price_in_xrd, Decimal::ONE);
            }

            (Decimal::ZERO, Decimal::ZERO)
        }

    }
}


#[derive(ScryptoSbor, ScryptoEvent)]
pub struct SupplyEvent {
    pub supply_token: ResourceAddress,
    pub supply_amount: Decimal,
    pub dx_token: ResourceAddress,
    pub dx_amount: Decimal
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct WithdrawEvent{
    pub dx_token: ResourceAddress,
    pub dx_amount: Decimal,
    pub withdraw_token: ResourceAddress,
    pub withdraw_amount: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct CreateCDPEvent{
    pub dx_token: ResourceAddress,
    pub dx_amount: Decimal,
    pub borrow_token: ResourceAddress,
    pub borrow_amount: Decimal,
    pub cdp_id: NonFungibleLocalId,
    pub is_stable: bool

}
#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ExtendBorrowEvent{
    pub borrow_token: ResourceAddress,
    pub amount: Decimal,
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct AdditionCollateralEvent{
    pub underlying_token: ResourceAddress,
    pub amount: Decimal,
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct WithdrawCollateralEvent{
    pub underlying_token: ResourceAddress,
    pub amount: Decimal,
    pub cdp_id: NonFungibleLocalId,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct RepayEvent{
    pub cdp_id: NonFungibleLocalId,
    pub repay_token: ResourceAddress,
    pub bucket_amount: Decimal,
    pub actual_payment: Decimal

}
