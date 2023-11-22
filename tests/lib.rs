use scrypto::prelude::*;
use scrypto_unit::*;
use transaction::builder::ManifestBuilder;

#[test]
fn test_hello() {
    // Setup the environment
    // let mut test_runner = TestRunner::builder().build();

    // // Create an account
    // let (public_key, _private_key, account_component) = test_runner.new_allocated_account();

    // // Publish package
    // let package_address = test_runner.compile_and_publish(this_package!());

    // // Test the `instantiate_hello` function.
    // let manifest = ManifestBuilder::new()
    //     .call_function(
    //         package_address,
    //         "Hello",
    //         "instantiate_hello",
    //         manifest_args!(),
    //     )
    //     .build();
    // let receipt = test_runner.execute_manifest_ignoring_fee(
    //     manifest,
    //     vec![NonFungibleGlobalId::from_public_key(&public_key)],
    // );
    // println!("{:?}\n", receipt);
    // let component = receipt.expect_commit(true).new_component_addresses()[0];

    // // Test the `free_token` method.
    // let manifest = ManifestBuilder::new()
    //     .call_method(component, "free_token", manifest_args!())
    //     .call_method(
    //         account_component,
    //         "deposit_batch",
    //         manifest_args!(ManifestExpression::EntireWorktop),
    //     )
    //     .build();
    // let receipt = test_runner.execute_manifest_ignoring_fee(
    //     manifest,
    //     vec![NonFungibleGlobalId::from_public_key(&public_key)],
    // );
    // println!("{:?}\n", receipt);
    // receipt.expect_commit_success();
}

#[test]
fn test_verify_ed25519(){
    // let pk = "d7feb0f5c5c1f587be6b651e3244da1b053e1aa3147c3219aa1aa1f6265e57a0";
    // let base = "resource_tdx_2_1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxtfd2jc";  //Runtime::bech32_encode_address(XRD);
    // let quote = "resource_tdx_2_1thnzj7nfawdec4tztfr9fmsh33kmwlstz2fs9t99yzj0awg43xxkpk";
    // let xrd = ResourceAddress::try_from_bech32(&AddressBech32Decoder::new(&NetworkDefinition::stokenet()), &base).unwrap();
    // let usdt = ResourceAddress::try_from_bech32(&AddressBech32Decoder::new(&NetworkDefinition::stokenet()), &quote).unwrap();
    // let epoch_at = 48484u64;
    // let timestamp = 1700642765u64;
    // let xrd_price_in_quote="0.0532645";
    // let signature = "0d61497a87a0700942683bae1ee209060c6802def4b4f11599dc4cc9a15859a094c131c4d4dc8294e4520e4415e331a403dfcb452c9ecb2a3cecd1bd4d04ac08";
    
    // let message = format!(
    //     "{base}/{quote}{price}{epoch_at}{timestamp}", base=base, quote=quote,
    //     price=xrd_price_in_quote, epoch_at=epoch_at, timestamp=timestamp
    // );
    // let msg_hash = hash(message);
    // info!("price message: {}, hash:{}", message.clone(), msg_hash);
    // let price_signer = Ed25519PublicKey::from_str(pk).unwrap();
    // if let Ok(sig) = Ed25519Signature::from_str(signature){
    //     if verify_ed25519(msg_hash, price_signer, sig){
    //         if let Ok(xrd_price_in_res) = Decimal::from_str(xrd_price_in_quote){
    //             // XRD/USDT --> USDT/XRD
    //             info!("xrd_price_in_res:{}: {}", base, Decimal::ONE.checked_div(xrd_price_in_res).unwrap());
    //         }
    //     }
    // }
}
