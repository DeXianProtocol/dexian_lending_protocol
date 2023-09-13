scrypto build
resim reset
result=$(resim new-account)
export admin_account=$(echo $result|grep "Account component address: "|awk -F ": " '{print $2}'|awk -F " " '{print $1}')
export admin_account_priv=$(echo $result|grep "Private key:" |awk -F "Private key: " '{print $2}'|awk -F " " '{print $1}')
export admin_account_badge=$(echo $result|grep "Owner badge: "|awk -F ": " '{print $5}'|awk -F " " '{print $1}')
export account=$admin_account
result=$(resim new-account)
export p1=$(echo $result|grep "Account component address: "|awk -F ": " '{print $2}'|awk -F " " '{print $1}')
export p1_priv=$(echo $result|grep "Private key:" |awk -F "Private key: " '{print $2}'|awk -F " " '{print $1}')
export p1_badge=$(echo $result|grep "Owner badge: "|awk -F ": " '{print $5}'|awk -F " " '{print $1}')
result=$(resim new-account)
export p2=$(echo $result|grep "Account component address: "|awk -F ": " '{print $2}'|awk -F " " '{print $1}')
export p2_priv=$(echo $result|grep "Private key:" |awk -F "Private key: " '{print $2}'|awk -F " " '{print $1}')
export p2_badge=$(echo $result|grep "Owner badge: "|awk -F ": " '{print $5}'|awk -F " " '{print $1}')
result=$(resim new-account)
export p3=$(echo $result|grep "Account component address: "|awk -F ": " '{print $2}'|awk -F " " '{print $1}')
export p3_priv=$(echo $result|grep "Private key:" |awk -F "Private key: " '{print $2}'|awk -F " " '{print $1}')
export p3_badge=$(echo $result|grep "Owner badge: "|awk -F ": " '{print $5}'|awk -F " " '{print $1}')


result=$(resim new-token-fixed --symbol=USDT --name=USDT 1000000)
# export usdt=$(echo $result | grep "Resource:" | awk -F " " '{print $3}')
export usdt=$(echo $result | awk -F "Resource: " '{print $2}')
result=$(resim new-token-fixed --symbol=USDC --name=USDC 1000000)
# export usdc=$(echo $result | grep "Resource:" | awk -F " " '{print $3}')
export usdc=$(echo $result | awk -F "Resource: " '{print $2}')
resim transfer $usdt:100000 $p2
resim transfer $usdc:100000 $p3
resim transfer $usdt:200 $p1
resim transfer $usdc:200 $p1




result=$(resim publish ".")
export pkg=$(echo $result | awk -F ": " '{print $2}')

result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_oracle.rtm)
export oracle=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_interest.rtm)
export def_interest_model=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')


result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_lending_factory.rtm)
export lending_component=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export admin_badge=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}' | awk -F " " '{print $1}')
export cdp=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $4}')


xrd="resource_sim1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxakj8n3"
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_xrd_pool.rtm)
export dx_xrd=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_usdt_pool.rtm)
export dx_usdt=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_usdc_pool.rtm)
export dx_usdc=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')


resim set-default-account $p1 $p1_priv $p1_badge
export supply_token=$xrd
export account=$p1
export amount=4000
resim run < ./docs/replace_holder.sh docs/transactions/supply.rtm

resim set-default-account $p2 $p2_priv $p2_badge
export supply_token=$xrd
export account=$p2
export amount=4000
resim run < ./docs/replace_holder.sh docs/transactions/supply.rtm

export supply_token=$usdt
export amount=200
resim run < ./docs/replace_holder.sh docs/transactions/supply.rtm

resim set-default-account $p3 $p3_priv $p3_badge
export account=$p3
export supply_token=$usdc
export amount=200
resim run < ./docs/replace_holder.sh docs/transactions/supply.rtm

resim set-default-account $p1 $p1_priv $p1_badge
export account=$p1
export dx_token=$dx_xrd
export amount=2000
export borrow_token=$usdt
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm
export borrow_token=$usdc
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm


resim set-default-account $p2 $p2_priv $p2_badge
export account=$p2
export dx_token=$dx_xrd
export amount=2000
export borrow_token=$usdt
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_stable.rtm
export borrow_token=$usdc
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_stable.rtm



resim call-method $lending_component 'get_interest_rate' $usdc
resim call-method $lending_component 'get_total_supply_borrow' $usdc
resim call-method $lending_component 'get_current_index' $usdc
resim call-method $lending_component 'get_current_supply_borrow' $usdc
resim call-method $lending_component 'get_ltv_and_liquidation_threshold' $usdc




resim call-method $oracle 'set_price_quote_in_xrd' $usdc 0.8833
resim call-method $oracle 'set_price_quote_in_xrd' $usdt 0.8833
resim set-current-epoch 15018
resim call-method $lending_component 'get_current_index' $usdc


resim call-method $lending_component 'repay' "$usdt:200" "$cdp:#1#"
resim show $p1
resim call-method $lending_component 'repay' "$usdc:200" "$cdp:#2#"



