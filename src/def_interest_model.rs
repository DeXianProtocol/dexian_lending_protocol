use scrypto::prelude::*;

#[derive(ScryptoSbor, Eq, PartialEq, Debug, Clone)]
pub enum InterestModel {
    Default,
    Stable
}

#[blueprint]
mod def_interest_model{
    struct DefInterestModel;
    

    impl DefInterestModel {

        pub fn new() -> Global<DefInterestModel>{
            Self{
            }.instantiate().prepare_to_globalize(OwnerRole::None).globalize()
        }

        pub fn get_borrow_interest_rate(&self, borrow_ratio: Decimal, model: InterestModel) -> Decimal{
            match model{
                InterestModel::Default => if borrow_ratio > Decimal::ONE {
                    // Decimal::ONE / Decimal::from("5") + Decimal::ONE * Decimal::ONE / Decimal::ONE / Decimal::from("2")
                    dec!("0.2") + dec!("0.5")
                }
                else{
                    // 0.2 * r + 0.5 * r**2
                    borrow_ratio * dec!("0.2") + borrow_ratio * borrow_ratio * dec!("0.5")
                },
                InterestModel::Stable => {
                    let x2 = if borrow_ratio > Decimal::ONE {
                        // let x = Decimal::ONE;
                        // x * x / Decimal::ONE;
                        Decimal::ONE
                    }
                    else{
                        borrow_ratio * borrow_ratio
                    };
                    let x4 = x2 * x2;
                    let x8 = x4 * x4;
                    dec!("0.55") * x4  + dec!("0.45")* x8
                }

            }
            
        }

        pub fn get_stable_interest_rate(&self, _borrow_ratio: Decimal, _stable_ratio: Decimal, _model: InterestModel) -> Decimal{
            dec!("0.05")
        }
    }


}