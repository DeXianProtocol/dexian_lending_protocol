use scrypto::prelude::*;
use crate::utils::verify_ed25519;

#[derive(ScryptoSbor, Clone, PartialEq, Debug)]
pub struct QuotePrice {
    pub price: Decimal,
    pub epoch_at: u64
}

#[blueprint]
#[events(SetPriceEvent, SetPublicKeyEvent, SetValidityPeriodEvent)]
mod oracle{

    enable_method_auth!{
        roles{
            operator => updatable_by: [];
            admin => updatable_by: [];
        },
        methods {
            //admin
            set_verify_public_key => restrict_to: [admin, OWNER];
    
            //op
            set_price_quote_in_xrd => restrict_to: [operator, admin];
            set_validity_period => restrict_to: [operator, admin]; 
    
            //public
            get_price_quote_in_xrd => PUBLIC;
            get_valid_price_in_xrd => PUBLIC;
    
        }
    }

    struct PriceOracle{
        price_map: HashMap<ResourceAddress, QuotePrice>,
        pk_str: String,
        last_validation_epoch: u64,
        last_validation_timestamp: u64,
        max_diff: u64,
    }
    
    impl PriceOracle{
        
        pub fn instantiate(
            owner_role: OwnerRole,
            op_rule: AccessRule,
            admin_rule: AccessRule,
            price_signer_pk: String,
            max_diff: u64
        ) -> Global<PriceOracle> {
            Self{
                price_map: HashMap::new(),
                pk_str: price_signer_pk.to_owned(),
                last_validation_epoch: 0u64,
                last_validation_timestamp: 0u64,
                max_diff
            }.instantiate().prepare_to_globalize(
                owner_role
            ).roles(
                roles!(
                    admin => admin_rule;
                    operator => op_rule;
                )
            )
            .globalize()
        }
    
        pub fn set_price_quote_in_xrd(&mut self, res_addr: ResourceAddress, price_in_xrd: Decimal){
            let epoch_at = Runtime::current_epoch().number();
            self.price_map.entry(res_addr).and_modify(|quote|{
                quote.price = price_in_xrd;
                quote.epoch_at = epoch_at;
            }).or_insert(QuotePrice { price: price_in_xrd, epoch_at });
            
            Runtime::emit_event(SetPriceEvent{price:price_in_xrd, res_addr});
        }

        pub fn set_validity_period(&mut self, validity_period_ms: u64){
            let previous = self.max_diff;
            self.max_diff = validity_period_ms;

            Runtime::emit_event(SetValidityPeriodEvent{new_value:validity_period_ms, previous});
        }

        pub fn set_verify_public_key(&mut self, price_signer_pk: String){
            // self.price_signer = Ed25519PublicKey::from_str(&price_signer_pk).unwrap();
            self.pk_str = price_signer_pk.to_owned();
            Runtime::emit_event(SetPublicKeyEvent{pub_key:price_signer_pk});
        }
    
        
        pub fn get_price_quote_in_xrd(&self, res_addr: ResourceAddress) -> Decimal {
            assert!(self.price_map.contains_key(&res_addr), "unknow resource address");
            let epoch_at = Runtime::current_epoch().number();
            let quote = self.price_map.get(&res_addr).unwrap();
            if quote.epoch_at == epoch_at{
                quote.price;
            }
            Decimal::ZERO
        }
    
        pub fn get_valid_price_in_xrd(&mut self, quote_addr: ResourceAddress, xrd_price_in_quote: String, timestamp: u64, signature: String) -> Decimal{
            assert!(self.price_map.contains_key(&quote_addr), "unknow resource address");
            // let epoch_at = 48538u64;  //Runtime::current_epoch().number();
            // let base = "resource_tdx_2_1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxtfd2jc";  //Runtime::bech32_encode_address(XRD);
            // let quote = "resource_tdx_2_1tkaegwwrttt6jrzvn2ag6dsvjs64dfwya6sckvlxnf794y462lhtx0";  //Runtime::bech32_encode_address(quote_addr);
            let epoch_at = Runtime::current_epoch().number();
            let base = Runtime::bech32_encode_address(XRD);
            let quote = Runtime::bech32_encode_address(quote_addr);
            let message = format!(
                "{base}/{quote}{price}{epoch_at}{timestamp}", base=base, quote=quote,
                price=xrd_price_in_quote, epoch_at=epoch_at, timestamp=timestamp
            );
            
            info!("price message: {}, signature:{}", message.clone(), signature.clone());
            if verify_ed25519(&message, &self.pk_str, &signature){
                if self.last_validation_epoch == epoch_at{
                    assert!((self.last_validation_timestamp as i128 - timestamp as i128) < self.max_diff as i128, "Price information has become too stale.");
                    if self.last_validation_timestamp < timestamp{
                        // keep latest timestamp
                        self.last_validation_timestamp = timestamp;
                    }
                }
                if self.last_validation_epoch < epoch_at {
                    //keep latest epoch
                    self.last_validation_epoch = epoch_at;
                    self.last_validation_timestamp = timestamp;
                }
                
                if let Ok(xrd_price_in_res) = Decimal::from_str(&xrd_price_in_quote){
                    info!("price verify passed. :)");
                    // XRD/USDT --> USDT/XRD
                    return Decimal::ONE.checked_div(xrd_price_in_res).unwrap();
                }
            }
            
            Decimal::ZERO 
        }

    }
}


#[derive(ScryptoSbor, ScryptoEvent)]
pub struct SetPriceEvent {
    pub res_addr: ResourceAddress,
    pub price: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct SetPublicKeyEvent{
    pub pub_key: String
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct SetValidityPeriodEvent{
    pub previous: u64,
    pub new_value: u64
}

