use scrypto::prelude::*;

const EPOCH_OF_YEAR: u64 = 105120;

#[derive(ScryptoSbor)]
struct StakeData{
    validator: Validator,
    last_staked: Decimal,
    last_stake_epoch: u64
}

#[blueprint]
mod validator_data_feed{

    struct ValidatorFeed{
        validator_map: HashMap<ComponentAddress, StakeData>,
        lsu_map: HashMap<ComponentAddress, Vault>,
        vault: Vault,
        last_staked: Decimal,
        last_stake_epoch: u64
    }

    impl ValidatorFeed {
        pub fn instantiate() -> (Global<ValidatorFeed>, Bucket){
            let admin_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                //set divisibility to none to ensure that the admin badge can not be fractionalized.
                .divisibility(DIVISIBILITY_NONE)
                .mint_initial_supply(Decimal::ONE);

            let component = self{
                validator_map: HashMap::new(),
                lsu_map: HashMap::new(),
                vault: Vault::new(XRD),
                last_staked: Decimal::ZERO,
                last_stake_epoch: 0u64
            }.instantiate()
            .prepare_to_globalize(
                OwnerRole::Fixed(rule!(require(admin_badge.resource_address())))
            ).roles(
                roles!(
                    admin => rule!(require(admin_badge.resource_address()));
                )
            ).globalize();         
            (component, admin_badge.into())
        }

        pub fn register_with_stake(&mut self, stake: Bucket, validator_addr: ComponentAddress) {
            assert!(XRD == stake.resource_address() && self.validator_map.contains_key(&validator_addr), "stake XRD only or validator_addr already register!");
            let validator: Validator = validator_addr.into();
            assert!(validator.accepts_delegated_stake(), "the validator does not accept delegate stake!");
            let current_epoch = Runtime::current_epoch().number();
            let current_stake = self.reset_current_staked();

            let stake_amount = stake.amount();
            let lsu_bucket = validator.stake(stake);
            let stake_data = StakeData{
                last_staked: stake_amount,
                last_stake_epoch: current_epoch,
                validator
            }
            self.validator_map.put(validator_addr, stake_data);
            self.lsu_map.put(validator_addr, Vault::with_bucket(lsu_bucket));

            self
        }

        fn reset_current_staked(&mut self) -> Decimal {
            let current_epoch = Runtime::current_epoch().number();
            if self.last_stake_epoch == current_epoch {
                return self.last_staked;
            }

            let mut sum = Decimal::ZERO;
            for (validator_addr, stake_data) in &self.validator_map{
                let latest = stake_data.validator.get_redemption_value(self.lsu_map.get(&validator_addr).unwrap().amount());
                stake_data.last_staked = latest;
                stake_data.last_stake_epoch = current_epoch;
                sum += latest;
            }
            
            sum
        }
    }

}