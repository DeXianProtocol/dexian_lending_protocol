use scrypto::prelude::*;


// trait InterestModel{
//     fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal;
// }

#[blueprint]
mod stable_interest_model{
    struct StableInterestModel{
        a: Decimal,
        b: Decimal
    }

    impl StableInterestModel{
        pub fn new() -> ComponentAddress{
            Self{
                a: Decimal::from(55),
                b: Decimal::from(45)
            }.instantiate().globalize()
        }

        pub fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal{
            let x2 = 
                if borrow_ratio > Decimal::ONE {
                    // let x = Decimal::ONE;
                    // x * x / Decimal::ONE;
                    Decimal::ONE
                }
                else{
                    borrow_ratio * borrow_ratio / Decimal::ONE
                };
            let x4 = x2 * x2 / Decimal::ONE;
            let x8 = x4 * x4 / Decimal::ONE;
            
            let hundred: Decimal = Decimal::from(100);
            // Decimal::from("55") * x4 / HUNDRED + Decimal::from("45") * x8 / hundred
            self.a * x4 / hundred + self.b * x8 / hundred
        }
    }    
}