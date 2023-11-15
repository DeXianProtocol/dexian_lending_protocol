
use scrypto::prelude::*;

/// Copies a slice to a fixed-sized array.
pub fn copy_u8_array<const N: usize>(slice: &[u8]) -> [u8; N] {
    if slice.len() == N {
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(slice);
        bytes
    } else {
        panic!("Invalid length: expected {}, actual {}", N, slice.len());
    }
}


pub fn ceil(dec: Decimal, divisibility: u8) -> Decimal{
    dec.checked_round(divisibility, RoundingMode::ToPositiveInfinity).unwrap()
}

pub fn floor(dec: Decimal, divisibility: u8) -> Decimal{
    dec.checked_round(divisibility, RoundingMode::ToNegativeInfinity).unwrap()
}

pub fn assert_resource(res_addr: &ResourceAddress, expect_res_addr: &ResourceAddress){
    assert!(res_addr == expect_res_addr, "the resource address is not expect!");
}

pub fn assert_vault_amount(vault: &Vault, not_less_than: Decimal){
    assert!(!vault.is_empty() && vault.amount() >= not_less_than, "the balance in vault is insufficient.");
}

pub fn assert_amount(v: Decimal, not_less_than: Decimal){
    assert!(v < not_less_than, "target value less than expect!");
}

pub fn calc_linear_interest(amount: Decimal, apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    amount.checked_mul(apy.checked_mul(delta_epoch).unwrap().checked_div(epoch_of_year).unwrap()).unwrap()
}

pub fn calc_compound_interest(amount: Decimal, apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    amount.checked_mul(
        Decimal::ONE.checked_add(
            apy.checked_div(epoch_of_year).unwrap()
        ).unwrap().checked_powi(delta_epoch as i64).unwrap()
    ).unwrap()
}

pub fn get_weight_rate(amount: Decimal, rate: Decimal, new_amount:Decimal, new_rate:Decimal) -> Decimal{
    let latest_amount = amount.checked_add(new_amount).unwrap();
    amount.checked_mul(rate).unwrap()
        .checked_add(new_amount.checked_mul(new_amount).unwrap()).unwrap()
        .checked_div(latest_amount).unwrap()
}

pub fn calc_principal(amount: Decimal,  apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    amount.checked_div(
        Decimal::ONE.checked_add(
            apy.checked_div(epoch_of_year).unwrap()
        ).unwrap().checked_powi(delta_epoch as i64).unwrap()
    ).unwrap()
}

pub fn get_divisibility(res_addr: ResourceAddress) -> Option<u8>{
    let res_mgr = ResourceManager::from(res_addr);
    res_mgr.resource_type().divisibility()
}