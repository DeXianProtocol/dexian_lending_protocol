use scrypto::prelude::*;

#[blueprint]
mod oracle{
    struct PriceOracle{
        usdc: ResourceAddress,
        usdt: ResourceAddress,
        usdc_price: Decimal,
        usdt_price: Decimal
    }

    impl PriceOracle{
        
        pub fn new(usdt: ResourceAddress, usdt_price: Decimal, usdc: ResourceAddress, usdc_price: Decimal) -> ComponentAddress {
            Self{
                usdc,
                usdt,
                usdc_price,
                usdt_price
            }.instantiate().globalize()
        }

        pub fn set_price_quote_in_xrd(&mut self, res_addr: ResourceAddress, price_in_xrd: Decimal){
            if res_addr == self.usdc {
                self.usdc_price = price_in_xrd;
            }
            else if res_addr == self.usdt {
                self.usdt_price = price_in_xrd;
            }
        }

        
        pub fn get_price_quote_in_xrd(&self, res_addr: ResourceAddress) -> Decimal {
            // Simulate changes in the market environment (time) to return different quotes
            // the actual application needs to use the real quote source and price
            // if Runtime::current_epoch() > 15017 {
            //     // match res_addr {
            //     //     RADIX_TOKEN => Decimal::ONE,
            //     //     self.usdc => Decimal::from("1.66666666"),  // 1/0.6
            //     //     self.usdt => Decimal::from("1.63934426"), // 1/0.61
            //     // }
            //     if res_addr == RADIX_TOKEN {
            //         Decimal::ONE
            //     }
            //     else if res_addr == self.usdc {
            //         Decimal::from("1.66666666") // 1/0.6
            //     }
            //     else if res_addr == self.usdt {
            //         Decimal::from("1.63934426")
            //     }
            //     else{
            //         Decimal::from("-1")
            //     }
            // }
            // else{
            //     // match res_addr{
            //     //     RADIX_TOKEN => Decimal::ONE,
            //     //     self.usdc => Decimal::from("16.66666666"),  // 1/0.6
            //     //     self.usdt => Decimal::from("16.39344262"),  // 1/0.61
            //     // }
            //     if res_addr == RADIX_TOKEN {
            //         Decimal::ONE
            //     }
            //     else if res_addr == self.usdc {
            //         Decimal::from("16.66666666")  // 1/0.6
            //     }
            //     else if res_addr == self.usdt {
            //         Decimal::from("16.39344262")
            //     }
            //     else{
            //         Decimal::from("-1")
            //     }
            // }

            if res_addr == RADIX_TOKEN {
                Decimal::ONE
            }
            else if res_addr == self.usdc {
                self.usdc_price
            }
            else if res_addr == self.usdt {
                self.usdt_price
            }
            else{
                Decimal::from(-1)
            }

        }
    }
}