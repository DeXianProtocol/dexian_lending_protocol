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
resim transfer $usdc:100000 $p2
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


export xrd="resource_sim1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxakj8n3"
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


resim show $lending_component

# borrow
resim set-default-account $p1 $p1_priv $p1_badge
export account=$p1
export dx_token=$dx_xrd
export amount=1000
export borrow_token=$usdt
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm
export borrow_token=$usdc
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm

resim show $lending_component
resim call-method $lending_component 'get_interest_rate' $usdt
resim call-method $lending_component 'get_interest_rate' $usdc

resim set-current-epoch 15019
# repay
resim show $p1

export account=$p1
export cdp_id=1
export repay_token=$usdt
export amount=100
resim run < ./docs/replace_holder.sh docs/transactions/repay.rtm

resim show $p1
resim show $lending_component
resim call-method $lending_component 'get_current_index' $usdt
resim call-method $lending_component 'get_interest_rate' $usdt
resim call-method $lending_component 'get_interest_rate' $usdc

resim set-current-epoch 30038

export repay_token=$usdc
export cdp_id=2
export amount=200
resim run < ./docs/replace_holder.sh docs/transactions/repay.rtm

resim show $p1
resim show $lending_component
resim call-method $lending_component 'get_current_index' $usdc
resim call-method $lending_component 'get_interest_rate' $usdt
resim call-method $lending_component 'get_interest_rate' $usdc


# borrow_stable
resim set-default-account $p2 $p2_priv $p2_badge
export account=$p2
export dx_token=$dx_xrd
export amount=1000
export borrow_token=$usdt
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_stable.rtm
export borrow_token=$usdc
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_stable.rtm

resim set-current-epoch 40056
# extend_borrow
export account=$p2
export cdp_id="#3#"
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/extend_borrow.rtm
export cdp_id="#4#"
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/extend_borrow.rtm

resim show $p2
resim show $lending_component
resim call-method $lending_component 'get_current_index' $usdc
resim call-method $lending_component 'get_interest_rate' $usdt
resim call-method $lending_component 'get_interest_rate' $usdc

resim set-current-epoch 50074
# repay
export account=$p2
export cdp_id=3
export repay_token=$usdt
export amount=100
resim run < ./docs/replace_holder.sh docs/transactions/repay.rtm
export repay_token=$usdc
export cdp_id=4
export amount=200
resim run < ./docs/replace_holder.sh docs/transactions/repay.rtm


# repay
export account=$p2
export cdp_id="#3#"
export amount=100
resim run < ./docs/replace_holder.sh docs/transactions/withdraw_collateral.rtm
export cdp_id="#4#"
export amount=1000
resim run < ./docs/replace_holder.sh docs/transactions/withdraw_collateral.rtm


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



===================
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
resim transfer $usdc:100000 $p2
resim transfer $usdc:100000 $p3
resim transfer $usdt:200 $p1
resim transfer $usdc:200 $p1


result=$(resim publish ".")
export pkg=$(echo $result | awk -F ": " '{print $2}')

result=$(resim call-function $pkg "ValidatorKeeper" "instantiate")
export keeper=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export admin_badge1=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}' | awk -F " " '{print $1}')
export op_badge1=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $3}' | awk -F " " '{print $1}')

result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_interest.rtm)
export def_interest_model=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')

export price_signer_pk=6d187b0f2e66d74410e92e2dc92a5141a55c241646ce87acbcad4ab413170f9b
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_lending_factory.rtm)
export lending_component=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export oracle=$(echo $result | grep "Component: "| awk -F "Component: " '{print $3}' | awk -F " " '{print $1}')
export cdp_mgr=$(echo $result | grep "Component: "| awk -F "Component: " '{print $4}' | awk -F " " '{print $1}')
export admin_badge=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}' | awk -F " " '{print $1}')
export op_badge=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $3}' | awk -F " " '{print $1}')
export cdp=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $4}' | awk -F " " '{print $1}')

resim run < ./docs/replace_holder.sh docs/transactions/set_price.rtm

export xrd="resource_sim1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxakj8n3"
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_xrd_pool.rtm)
export xrd_pool=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export dx_xrd=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}' | awk -F " " '{print $1}')
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_usdt_pool.rtm)
export usdt_pool=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export dx_usdt=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')
result=$(resim run < ./docs/replace_holder.sh docs/transactions/new_usdc_pool.rtm)
export usdc_pool=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export dx_usdc=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')


# supply
resim set-default-account $p1 $p1_priv $p1_badge
export supply_token=$xrd
export account=$p1
export amount=4000
resim run < ./docs/replace_holder.sh docs/transactions/supply.rtm

# export withdraw_token=$dx_xrd
# export account=$p1
# export amount=1000
# resim run < ./docs/replace_holder.sh docs/transactions/withdraw.rtm

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

resim show $xrd_pool
resim show $usdt_pool
resim show $usdc_pool



# borrow
# resim set-default-account $p1 $p1_priv $p1_badge
# export quote="usdt"
# result=$(curl "https://price.dexian.io/xrd-$quote")
# export price1=$(echo $result | jq ".data.price")
# export quote1=$usdt
# export timestamp1=$(echo $result | jq ".data.timestamp")
# export signature1=$(echo $result | jq ".data.signature")
# #export quote="usdc"
# #result=$(curl "https://price.dexian.io/xrd-$quote")
# #export price2=$(echo $result | jq ".data.price")
# #export quote2=$(echo "Address(\"${usdc}\")")
# #export timestamp2="$(echo $result | jq ".data.timestamp")u64"
# #export signature2=$(echo $result | jq ".data.signature")
# export price2=None
# export quote2=None
# export timestamp2=None
# export signature2=None
# export account=$p1
# export dx_token=$dx_xrd
# export amount=2000
# export borrow_token=$usdt
# export borrow_amount=10
# resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm


resim set-default-account $p1 $p1_priv $p1_badge
export quote="usdt"
export epoch=2
export price1="0.056259787085"
export quote1=$usdt
export timestamp1=1700658816
export signature1=$(python sign-util.py $xrd $usdt $price1 $epoch $timestamp1)
export price2=None
export quote2=None
export timestamp2=None
export signature2=None
export account=$p1
export dx_token=$dx_xrd
export amount=2000
export borrow_token=$usdt
export borrow_amount=10
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm

resim set-default-account $p1 $p1_priv $p1_badge
# export quote="usdt"
# result=$(curl "https://price.dexian.io/xrd-$quote")
# export price1=$(echo $result | jq ".data.price")
# export quote1=$usdt
# export timestamp1=$(echo $result | jq ".data.timestamp")
# export signature1=$(echo $result | jq ".data.signature")
# export price2=None
# export quote2=None
# export timestamp2=None
# export signature2=None
# export account=$p1
# export borrow_token=$usdt
# export borrow_amount=20
# export cdp_id="#1#"
# resim run < ./docs/replace_holder.sh docs/transactions/extend_borrow.rtm

export quote="usdt"
export epoch=2
export price1="0.056259787085"
export quote1=$usdt
export timestamp1=1700658816
export signature1=$(python sign-util.py $xrd $usdt $price1 $epoch $timestamp1)
export price2=None
export quote2=None
export timestamp2=None
export signature2=None
export account=$p1
export borrow_token=$usdt
export borrow_amount=20
export cdp_id="#1#"
resim run < ./docs/replace_holder.sh docs/transactions/extend_borrow.rtm


# export quote="usdt"
# result=$(curl "https://price.dexian.io/xrd-$quote")
# export price1=$(echo $result | jq ".data.price")
# export quote1=$usdt
# export timestamp1=$(echo $result | jq ".data.timestamp")
# export signature1=$(echo $result | jq ".data.signature")
# #export quote="usdc"
# #result=$(curl "https://price.dexian.io/xrd-$quote")
# #export price2=$(echo $result | jq ".data.price")
# #export quote2=$(echo "Address(\"${usdc}\")")
# #export timestamp2="Some($(echo $result | jq ".data.timestamp")u64)"
# #export signature2=$(echo $result | jq ".data.signature")
# export price2=None
# export quote2=None
# export timestamp2=None
# export signature2=None
# export account=$p1
# export amount=5
# export cdp_id="#1#"
# resim run < ./docs/replace_holder.sh docs/transactions/withdraw_collateral.rtm

export quote="usdt"
export epoch=2
export price1="0.056259787085"
export quote1=$usdt
export timestamp1=1700629817
export signature1=$(python sign-util.py $xrd $usdt $price1 $epoch $timestamp1)
export price2=None
export quote2=None
export timestamp2=None
export signature2=None
export account=$p1
export amount=5
export cdp_id="#1#"
resim run < ./docs/replace_holder.sh docs/transactions/withdraw_collateral.rtm

## support xrd and dx_xrd both
export addition_token=$xrd
export cdp_id="1"
export amount=1000
export account=$p1
resim run < ./docs/replace_holder.sh docs/transactions/addition_collateral.rtm


resim set-default-account $p3 $p3_priv $p3_badge
export quote="usdc"
result=$(curl "https://price.dexian.io/xrd-$quote")
export price1=$(echo $result | jq ".data.price")
export quote1=$usdc
export timestamp1=$(echo $result | jq ".data.timestamp")
export signature1=$(echo $result | jq ".data.signature")
#export quote="usdc"
#result=$(curl "https://price.dexian.io/xrd-$quote")
#export price2=$(echo $result | jq ".data.price")
#export quote2=$(echo "Address(\"${usdc}\")")
#export timestamp2="$(echo $result | jq ".data.timestamp")u64"
#export signature2=$(echo $result | jq ".data.signature")
export price2=None
export quote2=None
export timestamp2=None
export signature2=None
export account=$p3
export dx_token=$dx_usdc
export amount=200
export borrow_token=$xrd
export borrow_amount=2000
resim run < ./docs/replace_holder.sh docs/transactions/borrow_variable.rtm


export repay_token=$xrd
export amount=3000
export account=$p3
export cdp_id=2
resim run < ./docs/replace_holder.sh docs/transactions/repay.rtm
