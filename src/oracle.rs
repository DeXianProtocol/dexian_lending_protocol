use scrypto::prelude::*;
use crate::utils::verify_ed25519;

#[derive(ScryptoSbor, Clone, PartialEq, Debug)]
pub struct QuotePrice {
    pub price: Decimal,
    pub epoch_at: u64
}

#[blueprint]
#[events(SetPriceEvent, SetPublicKeyEvent)]
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
    
            //public
            get_price_quote_in_xrd => PUBLIC;
            get_valid_price_in_xrd => PUBLIC;
    
        }
    }

    struct PriceOracle{
        price_map: HashMap<ResourceAddress, QuotePrice>,
        // price_signer: Ed25519PublicKey,
        pk_str: String
    }
    
    impl PriceOracle{
        
        pub fn instantiate(
            owner_role: OwnerRole,
            op_rule: AccessRule,
            admin_rule: AccessRule,
            price_signer_pk: String
        ) -> Global<PriceOracle> {
            Self{
                price_map: HashMap::new(),
                // price_signer: Ed25519PublicKey::from_str(&price_signer_pk).unwrap()
                pk_str: price_signer_pk.to_owned()
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
    
        pub fn get_valid_price_in_xrd(&self, quote_addr: ResourceAddress, xrd_price_in_quote: String, timestamp: u64, signature: String) -> Decimal{
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
            let _hash = hash(message.clone());
            info!("price message: {}, hash:{}, signature:{}", message.clone(), _hash, signature.clone());
            if verify_ed25519(&message, &self.pk_str, &signature){
                info!("price verify passed. :)");
                if let Ok(xrd_price_in_res) = Decimal::from_str(&xrd_price_in_quote){
                    info!("price verify passed. :)");
                    // XRD/USDT --> USDT/XRD
                    return Decimal::ONE.checked_div(xrd_price_in_res).unwrap();
                }
            }
            else{  //TODO: only for test
                if let Ok(xrd_price_in_res) = Decimal::from_str(&xrd_price_in_quote){
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

