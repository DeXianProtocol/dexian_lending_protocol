use scrypto::prelude::*;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
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
            withdraw_fee => restrict_to: [operator, admin, OWNER];  // withdraw_fee should restrict to Pool?
        }
    }
    
    struct LendingProtocol{
        price_oracle: Global<PriceOracle>,
        pools: HashMap<ResourceAddress, Global<LendResourcePool>>,
        // address map for supply token(K) and deposit token(V), I.E. dxXRD --> XRD
        deposit_asset_map: HashMap<ResourceAddress, ResourceAddress>
    }

    impl LendingProtocol{

        pub fn instantiate(price_oracle: Global<PriceOracle>) -> (Global<LendingProtocol>, Bucket){
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

            let (address_reservation, component_address) =
            Runtime::allocate_component_address(LendingProtocol::blueprint_id());

            let owner_rule = rule!(require(admin_badge.resource_address()));
            let owner_role = OwnerRole::Fixed(owner_rule);
            let pool_mgr_rule = rule!(require(global_caller(component_address)));

            let component = Self{
                pools:HashMap::new(),
                deposit_asset_map: HashMap::new(),
                price_oracle
            }.instantiate()
            .prepare_to_globalize(owner_role)
            .with_address(address_reservation)
            .roles(roles! {
                admin => owner_rule;
                operator => rule!(require(admin_badge.resource_address()));
            })
            .globalize();
            
            (component, admin_badge.into())
        }

        pub fn new_pool(&mut self, 
            underlying_token_addr:ResourceAddress,
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal,
            interest_model: InterestModel,
            def_interest_model: Global<DefInterestModel>
        ) -> ResourceAddress{
            

        }

    }
}