use scrypto::prelude::*;


// trait InterestModel{
//     fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal;
// }

#[blueprint]
mod def_interest_model{
    struct DefInterestModel{
        a: Decimal,
        b: Decimal
    }

    impl DefInterestModel{

        pub fn new() -> ComponentAddress{
            Self{
                a: Decimal::from(5),
                b: Decimal::from(2)
            }.instantiate().globalize()
        }

        pub fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal{
            if borrow_ratio > Decimal::ONE {
                // Decimal::ONE / Decimal::from("5") + Decimal::ONE * Decimal::ONE / Decimal::ONE / Decimal::from("2")
                Decimal::ONE / self.a + Decimal::ONE * Decimal::ONE / Decimal::ONE / self.b
            }
            else{
                // 0.2 * r + 0.5 * r**2
                // borrow_ratio / Decimal::from("5") + borrow_ratio * borrow_ratio / Decimal::ONE / Decimal::from("2")
                borrow_ratio / self.a + borrow_ratio * borrow_ratio / Decimal::ONE / self.b
            }
        }
    }

    // impl InterestModel for DefInterestModel {

    //     pub fn get_borrow_interest_rate(&self, borrow_ratio: Decimal) -> Decimal{
    //         if borrow_ratio > Decimal::ONE {
    //             // Decimal::ONE / Decimal::from("5") + Decimal::ONE * Decimal::ONE / Decimal::ONE / Decimal::from("2")
    //             Decimal::ONE / self.a + Decimal::ONE * Decimal::ONE / Decimal::ONE / self.b
    //         }
    //         else{
    //             // borrow_ratio / Decimal::from("5") + borrow_ratio * borrow_ratio / Decimal::ONE / Decimal::from("2")
    //             borrow_ratio / self.a + borrow_ratio * borrow_ratio / Decimal::ONE / self.b
    //         }
    //     }

    // }

}