
use scrypto::prelude::*;
use ed25519_dalek::{PUBLIC_KEY_LENGTH, SIGNATURE_LENGTH, VerifyingKey, Signature};

pub const EPOCH_OF_YEAR: u64 = 105120;
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

pub fn precent_mul(dec: Decimal, precent: Decimal) -> Decimal{
    dec.checked_mul(precent).unwrap().checked_div(Decimal::ONE_HUNDRED).unwrap()
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
    let linear_rate = calc_linear_rate(apy, epoch_of_year, delta_epoch);
    amount.checked_mul(Decimal::ONE.checked_add(linear_rate).unwrap()).unwrap()
}

pub fn calc_linear_rate(apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    apy.checked_mul(delta_epoch).unwrap().checked_div(epoch_of_year).unwrap()
}

pub fn calc_compound_interest(amount: Decimal, apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    amount.checked_mul(calc_compound_rate(apy, epoch_of_year, delta_epoch)).unwrap()
}

/// (1+apy/epoch_of_year)^delta_epoch
pub fn calc_compound_rate(apy: Decimal, epoch_of_year: Decimal, delta_epoch: u64) -> Decimal{
    Decimal::ONE.checked_add(
        apy.checked_div(epoch_of_year).unwrap()
    ).unwrap().checked_powi(delta_epoch as i64).unwrap()
}

pub fn get_weight_rate(amount: Decimal, rate: Decimal, new_amount:Decimal, new_rate:Decimal) -> Decimal{
    let latest_amount = amount.checked_add(new_amount).unwrap();
    amount.checked_mul(rate).unwrap()
        .checked_add(new_amount.checked_mul(new_rate).unwrap()).unwrap()
        .checked_div(latest_amount).unwrap()
}

#[warn(dead_code)]
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

pub fn ceil_by_resource(res_addr: ResourceAddress, amount: Decimal) -> Decimal{
    let divisibility = get_divisibility(res_addr).unwrap();
    ceil(amount, divisibility)
}

pub fn floor_by_resource(res_addr: ResourceAddress, amount: Decimal) -> Decimal{
    let divisibility = get_divisibility(res_addr).unwrap();    
    floor(amount, divisibility)
}


pub fn verify_ed25519(
    msg: &str,
    pk: &str,
    sig: &str
) -> bool{
    let sig_bytes =  hex::decode(sig).expect("Failed to decode signature string");
    let signature = Signature::from_bytes(&copy_u8_array::<SIGNATURE_LENGTH>(&sig_bytes));
    let pk_bytes = hex::decode(pk).expect("Failed to decode public-key string");
    let public_key = VerifyingKey::from_bytes(&copy_u8_array::<PUBLIC_KEY_LENGTH>(&pk_bytes)).expect("Failed construct public-key.");
    public_key.verify_strict(msg.as_bytes(), &signature).is_ok()
}