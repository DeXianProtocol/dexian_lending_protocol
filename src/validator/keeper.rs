use scrypto::prelude::*;
use crate::utils::EPOCH_OF_YEAR;

pub const BABYLON_START_EPOCH: u64 = 0; //32718; // //mainnet: 32718, stokenet: 0
pub const A_WEEK_EPOCHS: u64 = 60/5*24*7;
pub const RESERVE_WEEKS: usize = 52;

#[derive(Debug, Clone, Copy, ScryptoSbor)]
pub struct StakeData{
    pub last_lsu: Decimal,
    pub last_staked: Decimal,
    pub last_stake_epoch: u64
}


#[derive(Debug, Clone, PartialEq, Eq, ScryptoSbor, NonFungibleData)]
pub struct UnstakeData {
    pub name: String,

    /// An epoch number at (or after) which the pending unstaked XRD may be claimed.
    /// Note: on unstake, it is fixed to be [`ConsensusManagerConfigSubstate.num_unstake_epochs`] away.
    pub claim_epoch: Epoch,

    /// An XRD amount to be claimed.
    pub claim_amount: Decimal,
}


#[blueprint]
//#[events(DebugGetApy, DebugGetApy2)]
mod validator_keeper{

    enable_method_auth!{
        roles{
            operator => updatable_by: [];
            admin => updatable_by: [];
        },
        methods {
            // admin
            fill_validator_staking => restrict_to: [admin, OWNER];
            log_validator_staking => restrict_to: [admin, OWNER];
            insert_validator_staking => restrict_to: [admin, OWNER];
            // op
            register_validator_address => restrict_to: [operator, OWNER];

            // public
            get_active_set_apy => PUBLIC;
            get_validator_address => PUBLIC;

        }
    }

    struct ValidatorKeeper{
        validator_map: HashMap<ComponentAddress, Vec<StakeData>>,
        res_validator_map: KeyValueStore<ResourceAddress, ComponentAddress>
    }

    impl ValidatorKeeper {
        pub fn instantiate(
            // owner_rule: AccessRule,
            // admin_rule: AccessRule,
            // op_rule: AccessRule
        ) -> (Global<ValidatorKeeper>, Bucket, Bucket){
            let admin_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata!(
                    init {
                        "name" => "Keeper Admin Badge".to_owned(), locked;
                        "description" => 
                        "This is a DeXian Lending Protocol admin badge used to authenticate the admin.".to_owned(), locked;
                    }
                ))
                .mint_initial_supply(1);
            let op_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata!(
                    init {
                        "name" => "Keeper Operator Badge".to_owned(), locked;
                        "description" => 
                        "This is a DeXian Lending Protocol operator badge used to authenticate the operator.".to_owned(), locked;
                    }
                ))
                .mint_initial_supply(1);
            let admin_rule = rule!(require(admin_badge.resource_address()));
            let op_rule = rule!(require(op_badge.resource_address()));
            let owner_rule = rule!(require(admin_badge.resource_address()));
            
            let component = Self{
                validator_map: HashMap::new(),
                res_validator_map: KeyValueStore::new()
            }.instantiate()
            // .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge.resource_address()))))
            .prepare_to_globalize(OwnerRole::Fixed(owner_rule))
            .roles(
                roles!(
                    admin => admin_rule;
                    operator => op_rule;
                )
            ).globalize();
            
            (component, admin_badge.into(), op_badge.into())
        }

        
        
        pub fn register_validator_address(&mut self, res_addr_vec: Vec<ResourceAddress>, validator_addr_vec: Vec<ComponentAddress>){
            assert_eq!(res_addr_vec.capacity(), validator_addr_vec.capacity(), "The resource addresses must matches validator addresses!");
            for (res_addr, validator_addr) in res_addr_vec.iter().zip(validator_addr_vec.iter()) {
                self.res_validator_map.insert(res_addr.clone(), validator_addr.clone());
            }
        }

        // /// get validator by LSU address or ClaimNFT address
        pub fn get_validator_address(&self, res_addr: ResourceAddress) -> ComponentAddress{
            assert!(self.res_validator_map.get(&res_addr).is_some(), "unknow resource address");
            self.res_validator_map.get(&res_addr).unwrap().clone()
        }

        pub fn fill_validator_staking(&mut self, validator_addr: ComponentAddress, stake_data_vec: Vec<StakeData>){
            self.validator_map.entry(validator_addr).or_insert(stake_data_vec.clone());
            info!("{}: {},{},{}", Runtime::bech32_encode_address(validator_addr), stake_data_vec[0].last_lsu, stake_data_vec[0].last_staked, stake_data_vec[0].last_stake_epoch);
        }

        pub fn insert_validator_staking(&mut self, validator_addr: ComponentAddress, index:usize,  stake_data: StakeData){
            assert!(self.validator_map.contains_key(&validator_addr), "unknown validator");
            self.validator_map.get_mut(&validator_addr).unwrap().insert(index, stake_data);
        }


        pub fn log_validator_staking(&mut self, add_validator_list: Vec<ComponentAddress>, remove_validator_list: Vec<ComponentAddress>) {
            let current_epoch = Runtime::current_epoch().number();
            let current_week_index = Self::get_week_index(current_epoch);
        
            // Remove validators from the map
            remove_validator_list.iter().for_each(|remove_validator_addr| {
                self.validator_map.remove(remove_validator_addr);
            });
        
            // Update staking information for existing validators
            let mut current_staked = self.validator_map.iter_mut()
            .map(|(validator_addr, vec)| {
                let validator: Global<Validator> = Global::from(validator_addr.clone());
                let last_lsu = validator.total_stake_unit_supply();
                let last_staked = validator.total_stake_xrd_amount();
                let latest = vec.first_mut().unwrap();
                let last_index = Self::get_week_index(latest.last_stake_epoch);
                if current_week_index > last_index {
                    vec.insert(0, Self::new_stake_data(last_lsu, last_staked, current_epoch));
                    while vec.capacity() > RESERVE_WEEKS {
                        vec.remove(vec.capacity()-1);
                    }
                }
                else{
                    latest.last_lsu = last_lsu;
                    latest.last_staked = last_staked;
                    latest.last_stake_epoch = current_epoch;
                }
                last_staked
            })
            .fold(Decimal::ZERO, |sum, staked| {
                sum.checked_add(staked).unwrap()
            });

            // Add new validators and update their staking information
            add_validator_list.iter().for_each(|add_validator_addr| {
                if !self.validator_map.contains_key(add_validator_addr) {
                    let staked = self.set_validator_staking(add_validator_addr, current_week_index, current_epoch);
                    current_staked = current_staked.checked_add(staked).unwrap();
                }
            });

        }
        

        fn set_validator_staking(&mut self, validator_addr: &ComponentAddress, current_week_index: usize, current_epoch: u64) -> Decimal{
            let validator: Global<Validator> = Global::from(validator_addr.clone());
            let last_lsu = validator.total_stake_unit_supply();
            let last_staked = validator.total_stake_xrd_amount();
            self.validator_map.entry(validator_addr.clone()).and_modify(|vec|{
                let latest = vec.first_mut().unwrap();
                let last_index = Self::get_week_index(latest.last_stake_epoch);
                if current_week_index > last_index {
                    vec.insert(0, Self::new_stake_data(last_lsu, last_staked, current_epoch));
                    while vec.capacity() > RESERVE_WEEKS {
                        vec.remove(vec.capacity()-1);
                    }
                }
                else{
                    latest.last_lsu = last_lsu;
                    latest.last_staked = last_staked;
                    latest.last_stake_epoch = current_epoch;
                } 

            }).or_insert(Vec::from([Self::new_stake_data(last_lsu, last_staked, current_epoch)]));
            
            last_staked
        }

        fn new_stake_data(last_lsu: Decimal, last_staked: Decimal, last_stake_epoch: u64) -> StakeData{
            StakeData{
                last_stake_epoch,
                last_lsu,
                last_staked
            }
        }

        fn get_week_index(epoch_at: u64) -> usize{
            // let index: I192 = Decimal::from(epoch_at - BABYLON_START_EPOCH).checked_div(Decimal::from(A_WEEK_EPOCHS)).unwrap()
            // .checked_ceiling().unwrap().try_into();
            // ().to_usize()
            let elapsed_epoch = epoch_at - BABYLON_START_EPOCH;
            let week_index = elapsed_epoch / A_WEEK_EPOCHS;
            let ret =  if week_index * A_WEEK_EPOCHS < elapsed_epoch{
                (week_index + 1) as usize
            }
            else{
                week_index as usize
            };
            ret
        }

        pub fn get_active_set_apy(&self) -> Decimal {
            let current_epoch = Runtime::current_epoch().number();
            let current_week_index = Self::get_week_index(current_epoch);
        
            let (sum, count) = self.validator_map.iter()
                .filter_map(|(validator_addr, vec)| {
                    self.get_validator_apy(validator_addr, vec, current_week_index)
                })
                .fold((Decimal::ZERO, Decimal::ZERO), |(sum, count), apy| {
                    (sum + apy, count + Decimal::ONE)
                });
            info!("sum:{}, count:{}", sum, count);
            // Runtime::emit_event(DebugGetApy2{
            //     sum,
            //     count
            // });
            if count.is_zero() {
                Decimal::ZERO
            } else {
                sum  / count
            }
        }
        

        fn get_validator_apy(&self, _validator_addr: &ComponentAddress, vec: &Vec<StakeData>, current_week_index: usize) -> Option<Decimal> {
            let latest = vec.first()?;
            let latest_week_index = Self::get_week_index(latest.last_stake_epoch);
        
            // The last entry must be within the last week (inclusive).
            if latest_week_index < current_week_index -1 {
                // Runtime::emit_event(DebugGetApy{
                //     validator: _validator_addr.clone(),
                //     last_epoch: latest.last_stake_epoch,
                //     latest_index: Decimal::ZERO,
                //     previous_index: Decimal::ZERO,
                //     delta_epoch: Decimal::ZERO,
                //     current_week_index,
                //     latest_week_index
                // });
                return None;
            }
        
            if let Some(previous) = vec.get(1) {
                let previous_week_index = Self::get_week_index(previous.last_stake_epoch);
        
                if previous_week_index == latest_week_index - 1 {
                    let latest_index = latest.last_staked.checked_div(latest.last_lsu)?;
                    let previous_index = previous.last_staked.checked_div(previous.last_lsu)?;
                    let delta_index = latest_index.checked_sub(previous_index)?;
                    let delta_epoch = Decimal::from(latest.last_stake_epoch - previous.last_stake_epoch);
                    // Runtime::emit_event(DebugGetApy{
                    //     validator: _validator_addr.clone(),
                    //     last_epoch: latest.last_stake_epoch,
                    //     current_week_index,
                    //     latest_week_index,
                    //     latest_index,
                    //     previous_index,
                    //     delta_epoch
                    // });
                    return Some(
                        (delta_index).checked_mul(Decimal::from(EPOCH_OF_YEAR)).unwrap()
                        .checked_div(delta_epoch).unwrap()
                    );
                }
            }
        
            None
        }

    }

}

// #[derive(ScryptoSbor, ScryptoEvent)]
// pub struct DebugGetApy{
//     pub validator: ComponentAddress,
//     pub last_epoch: u64,
//     pub current_week_index: usize,
//     pub latest_week_index: usize,
//     pub latest_index: Decimal,
//     pub previous_index: Decimal,
//     pub delta_epoch: Decimal
// }

// #[derive(ScryptoSbor, ScryptoEvent)]
// pub struct DebugGetApy2{
//     pub sum: Decimal,
//     pub count: Decimal
// }