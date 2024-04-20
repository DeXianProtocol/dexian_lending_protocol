use scrypto::prelude::*;
use crate::interest::InterestModel;
use crate::oracle::oracle::PriceOracle;
// use crate::cdp::CollateralDebtPosition;
use crate::cdp::FlashLoanData;
use crate::cdp::cdp_mgr::CollateralDebtManager;
use crate::earning::staking_earning::StakingEarning;
use crate::validator::keeper::validator_keeper::ValidatorKeeper;


#[blueprint]
#[events(SupplyEvent, WithdrawEvent, CreateCDPEvent, ExtendBorrowEvent, AdditionCollateralEvent, WithdrawCollateralEvent, RepayEvent, LiquidationEvent, FlashLoanEvent)]
mod dexian_protocol{

    enable_method_auth! {
        roles{
            admin => updatable_by: [];
            operator => updatable_by: [admin];
        },
        methods {
            // pool
            new_pool => restrict_to: [admin, OWNER];
            withdraw_insurance => restrict_to: [operator, OWNER];

            //lending
            supply => PUBLIC;
            withdraw => PUBLIC;
            borrow_variable => PUBLIC;
            borrow_stable => PUBLIC;
            extend_borrow => PUBLIC;
            withdraw_collateral => PUBLIC;
            repay => PUBLIC;
            addition_collateral => PUBLIC;
            liquidation => PUBLIC;

            //flashloan
            // migrate_cdp => PUBLIC;
            borrow_flashloan => PUBLIC;
            repay_flashloan => PUBLIC;

            //staking earning
            join => PUBLIC;
            redeem => PUBLIC;
        }
    }
    
    struct DeXianProtocol{
        price_oracle: Global<PriceOracle>,
        cdp_mgr: Global<CollateralDebtManager>,
        staking_mgr: Global<StakingEarning>,
        dse_res_addr: ResourceAddress,
        cdp_res_addr: ResourceAddress,
        admin_rule: AccessRule,
        op_rule: AccessRule,
    }

    impl DeXianProtocol{

        pub fn instantiate(
            validator_keeper: Global<ValidatorKeeper>,
            admin_rule: AccessRule,
            op_res_addr: ResourceAddress,
            price_signer_pk: String, 
            price_validity_ms: u64,
            unstake_epoch_num: u64,
            settle_gas: Decimal
        ) -> (
            Global<DeXianProtocol>,
            Global<PriceOracle>,
            Global<StakingEarning>, 
            ResourceAddress,
            ResourceAddress
        ){
            let (address_reservation, component_address) =
            Runtime::allocate_component_address(DeXianProtocol::blueprint_id());

            let price_oracle = PriceOracle::instantiate(
                OwnerRole::Fixed(admin_rule.clone()),
                rule!(require(op_res_addr)),
                admin_rule.clone(),
                price_signer_pk,
                price_validity_ms
            );

            let mgr_rule = rule!(require(op_res_addr) || require(global_caller(component_address)));
            let staking_mgr = StakingEarning::instantiate(
                validator_keeper,
                unstake_epoch_num,
                settle_gas,
                admin_rule.clone(),
                mgr_rule.clone()
            );
            
            
            let caller_rule = rule!(require(global_caller(component_address)) || require(global_caller(staking_mgr.address())));
            let (cdp_mgr, cdp_res_addr) = CollateralDebtManager::instantiate(
                admin_rule.clone(),
                mgr_rule.clone(),
                caller_rule,
                price_oracle
            );
            
            let dse_res_addr = staking_mgr.get_dse_token();
            let component = Self{
                admin_rule: admin_rule.clone(),
                op_rule: rule!(require(op_res_addr)),
                dse_res_addr,
                price_oracle,
                staking_mgr,
                cdp_mgr,
                cdp_res_addr
            }.instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(admin_rule.clone()))
            .with_address(address_reservation)
            .roles(roles! {
                admin => admin_rule.clone();
                operator => rule!(require(op_res_addr));
            })
            .globalize();
            
            (component, price_oracle, staking_mgr, dse_res_addr, cdp_res_addr)
        }

        pub fn new_pool(&mut self,
            share_divisibility: u8,
            underlying_token_addr: ResourceAddress,
            interest_model: InterestModel,
            interest_model_cmp_addr: ComponentAddress,
            ltv: Decimal,
            liquidation_threshold: Decimal,
            liquidation_bonus: Decimal,
            insurance_ratio: Decimal,
            flashloan_fee_ratio: Decimal
        ) -> ResourceAddress {
            self.cdp_mgr.new_pool(
                share_divisibility,
                underlying_token_addr,
                interest_model, 
                interest_model_cmp_addr, 
                ltv, 
                liquidation_threshold, 
                liquidation_bonus, 
                insurance_ratio, 
                flashloan_fee_ratio,
                self.admin_rule.clone(), 
                if underlying_token_addr == XRD {
                    Some(self.staking_mgr.address().into())
                }
                else{
                    None
                }
            )
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
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.extra_params(dx_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            info!("borrow_price_in_xrd:{}, collateral_underlying_price_in_xrd:{}",borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            assert!(borrow_price_in_xrd.is_positive() && collateral_underlying_price_in_xrd.is_positive(), "Incorrect information on price signature.");
            info!("collateral {}, amount:{}; price:{}/{}", Runtime::bech32_encode_address(dx_token), dx_amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            let (borrow_bucket, cdp_bucket) = self.cdp_mgr.borrow_variable(dx_bucket, borrow_token, borrow_amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
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
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.extra_params(dx_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            assert!(borrow_price_in_xrd.is_positive() && collateral_underlying_price_in_xrd.is_positive(), "Incorrect information on price signature.");
            let (borrow_bucket, cdp_bucket) = self.cdp_mgr.borrow_stable(dx_bucket, borrow_token, borrow_amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
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
            assert!(borrow_price_in_xrd.is_positive() && collateral_underlying_price_in_xrd.is_positive(), "Incorrect information on price signature.");
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
            assert!(borrow_price_in_xrd.is_positive() && collateral_underlying_price_in_xrd.is_positive(), "Incorrect information on price signature.");
            let (underlying_bucket, cdp_bucket) = self.cdp_mgr.withdraw_collateral(cdp, amount, borrow_price_in_xrd, collateral_underlying_price_in_xrd);
            Runtime::emit_event(WithdrawCollateralEvent{underlying_token:collateral_underlying_token, amount:underlying_bucket.amount(), cdp_id:cdp_id.clone()});
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

        pub fn liquidation(&mut self,
            debt_bucket: Bucket,
            debt_to_cover: Decimal,
            id: u64,
            price1: String,
            quote1: ResourceAddress,
            timestamp1: u64,
            signature1: String,
            price2: Option<String>,
            quote2: Option<ResourceAddress>,
            timestamp2: Option<u64>,
            signature2: Option<String>
        ) -> (Bucket, Bucket){
            let bucket_amount = debt_bucket.amount();
            let cdp_id = NonFungibleLocalId::integer(id);
            let (borrow_token, collateral_underlying_token) = self.cdp_mgr.get_cdp_resource_address(cdp_id.clone());
            assert!(borrow_token == debt_bucket.resource_address(), "the borrow token does not matches CDP.");
            let (borrow_price_in_xrd, collateral_underlying_price_in_xrd) = self.get_price_in_xrd(collateral_underlying_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2);
            assert!(borrow_price_in_xrd.is_positive() || collateral_underlying_price_in_xrd.is_positive(), "Incorrect information on price signature.");

            let (underlying_bucket, refund_bucket) = self.cdp_mgr.liquidation(debt_bucket, debt_to_cover, cdp_id.clone(), borrow_price_in_xrd, collateral_underlying_token, collateral_underlying_price_in_xrd);
            let underlying_amount = underlying_bucket.amount();
            let actual_repayment = bucket_amount.checked_sub(refund_bucket.amount()).unwrap();
            info!("underlying:{}, actual_repayment:{}", underlying_amount, actual_repayment);
            Runtime::emit_event(LiquidationEvent{
                cdp_id: cdp_id.clone(),
                debt_token: borrow_token,
                debt_price: borrow_price_in_xrd,
                underlying_token: collateral_underlying_token,
                underlying_price: collateral_underlying_price_in_xrd,
                underlying_amount,
                actual_repayment,
                debt_to_cover
            });
            (underlying_bucket,refund_bucket)
        }

        // pub fn migrate_cdp(&mut self, 
        //     cdp: Bucket, id: u64, repay_amount: Decimal, withdraw_collateral_amount: Decimal,
        //     price1: String, quote1: ResourceAddress, timestamp1: u64, signature1: String,
        //     price2: Option<String>, quote2: Option<ResourceAddress>, timestamp2: Option<u64>, signature2: Option<String>
        // )->(Bucket, Bucket, Bucket){
        //     let mut lending_factory : Global<LendingProtocol> = global_component!(
        //         LendingProtocol,
        //         "component_sim1cq7t704hs8memnzk0yevz3qfsvwp4hk04spy0apt8u7ks0j69wgyl2"
        //     );
            
        //     let cdp_id = cdp.as_non_fungible().non_fungible_local_id();
        //     let cdp_res_mgr = ResourceManager::from(cdp.resource_address());
        //     let cdp_data = cdp_res_mgr.get_non_fungible_data::<CollateralDebtPosition>(&cdp_id);
        //     let borrow_token = cdp_data.borrow_token.clone();
        //     let (repay_bucket, flashloan_bucket) = self.cdp_mgr.borrow_flashloan(borrow_token, repay_amount);
        //     info!("cdp_data{}, borrow_normalized:{} repay_bucket.amount:{}", Runtime::bech32_encode_address(borrow_token), cdp_data.normalized_borrow, repay_bucket.amount());
        //     let _remain_bucket = lending_factory.repay(repay_bucket, id);
        //     info!("repay: _remain_bucket.amount {}", _remain_bucket.amount());
        //     let (collateral_bucket, cdp_bucket) = lending_factory.withdraw_collateral(cdp, withdraw_collateral_amount, price1.clone(), quote1, timestamp1, signature1.clone(), price2.clone(), quote2, timestamp2, signature2.clone());
        //     info!("withdraw_collateral: collateral_bucket.amount {}", collateral_bucket.amount());
        //     let dx_bucket = self.cdp_mgr.supply(collateral_bucket);
        //     info!("supply: dx_bucket.amount {}", dx_bucket.amount());
        //     let (borrow_bucket, new_cdp) = self.borrow_variable(dx_bucket, borrow_token, repay_amount, price1, quote1, timestamp1, signature1, price2, quote2, timestamp2, signature2);
        //     info!("borrow_bucket: borrow_bucket.amount {}", borrow_bucket.amount());
        //     let flashloan_remain = self.repay_flashloan(borrow_bucket, flashloan_bucket);
        //     info!("flashloan_remain: flashloan_remain.amount {}", flashloan_remain.amount());
        //     (flashloan_remain, cdp_bucket, new_cdp)
        // }

        pub fn borrow_flashloan(&mut self, res_addr: ResourceAddress, amount: Decimal) -> (Bucket, Bucket){
            self.cdp_mgr.borrow_flashloan(res_addr, amount)
        }

        pub fn repay_flashloan(&mut self, repay_bucket: Bucket, flashloan: Bucket) -> Bucket{
            let nft_id: NonFungibleLocalId = flashloan.as_non_fungible().non_fungible_local_id();
            let flashloan_data = ResourceManager::from_address(flashloan.resource_address()).get_non_fungible_data::<FlashLoanData>(&nft_id);
            Runtime::emit_event(FlashLoanEvent{
                res_addr: flashloan_data.res_addr,
                bucket_amount: repay_bucket.amount(),
                amount: flashloan_data.amount,
                fee: flashloan_data.fee,
                nft_addr: flashloan.resource_address(),
                nft_id
            });
            self.cdp_mgr.repay_flashloan(repay_bucket, flashloan)
        }

        pub fn withdraw_insurance(&mut self, underlying_token_addr: ResourceAddress, amount: Decimal) -> Bucket{
            self.cdp_mgr.withdraw_insurance(underlying_token_addr, amount)
        }

        pub fn join(&mut self, validator: ComponentAddress, xrd_bucket: Bucket) -> Bucket{
            self.staking_mgr.join(validator, xrd_bucket)
        }

        pub fn redeem(&mut self, validator: ComponentAddress, bucket: Bucket, is_faster: bool) ->Bucket{
            self.staking_mgr.redeem(self.cdp_mgr, validator, bucket, is_faster)
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
        ) -> (Decimal, Decimal){
            let collateral_underlying_token = self.cdp_mgr.get_underlying_token(dx_token);
            self.get_price_in_xrd(collateral_underlying_token, borrow_token, &price1, quote1, timestamp1, &signature1, price2, quote2, timestamp2, signature2)
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

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct LiquidationEvent{
    pub cdp_id: NonFungibleLocalId,
    pub debt_token: ResourceAddress,
    pub debt_price: Decimal,
    pub debt_to_cover: Decimal,
    pub actual_repayment: Decimal,
    pub underlying_token: ResourceAddress,
    pub underlying_price: Decimal,
    pub underlying_amount: Decimal
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct FlashLoanEvent{
    pub res_addr: ResourceAddress,
    pub bucket_amount: Decimal,
    pub amount: Decimal,
    pub fee: Decimal,
    pub nft_addr: ResourceAddress,
    pub nft_id: NonFungibleLocalId
}
