use scrypto::prelude::*;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
use crate::pools::lending::lend_pool::LendResourcePool;
use crate::interest::def_interest_model::DefInterestModel;

#[blueprint]
mod dexian_lending{
    use crate::cdp::cdp_mgr::CollateralDebtManager;

    


    enable_method_auth! {
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [admin];
        },
        methods {
            new_pool => restrict_to: [admin, OWNER];
            withdraw_insurance => restrict_to: [operator, admin, OWNER];  // withdraw_fee should restrict to Pool?
        }
    }
    
    struct LendingProtocol{
        price_oracle: Global<PriceOracle>,
        cdp_mgr: Global<CollateralDebtManager>,
        cdp_res_addr: ResourceAddress
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
                OwnerRole::Fixed(rule!(require(admin_badge.resource_address()))), 
                cdp_mgr_rule, 
                price_oracle
            );

            let component = Self{
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

        pub fn new_pool(&mut self){

        }

        pub fn withdraw_insurance(&mut self){

        }

    }
}