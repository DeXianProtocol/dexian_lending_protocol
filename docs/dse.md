## Validator

#### 数据结构

last_sum_staked: 
last_update_epoch:
lsu_map: Map<validator_address, LSU>
validator_stake: Map<validator_address, StakeData>


#### register
1. reset each validator stake
2. sum validator stake
3. validator.stake
3. construct a StakeData, put it in