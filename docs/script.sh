scrypto build
resim reset
result=$(resim new-account)
export admin=$(echo $result|grep "Account component address: "|awk -F ": " '{print $2}'|awk -F " " '{print $1}')
export admin_priv=$(echo $result|grep "Private key:" |awk -F "Private key: " '{print $2}'|awk -F " " '{print $1}')
export admin_badge=$(echo $result|grep "Owner badge: "|awk -F ": " '{print $5}'|awk -F " " '{print $1}')
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


result=$(resim new-token-fixed --symbol=USDT 1000000)
# export usdt=$(echo $result | grep "Resource:" | awk -F " " '{print $3}')
export usdt=$(echo $result | awk -F "Resource: " '{print $2}')
result=$(resim new-token-fixed --symbol=USDC 1000000)
# export usdc=$(echo $result | grep "Resource:" | awk -F " " '{print $3}')
export usdc=$(echo $result | awk -F "Resource: " '{print $2}')





result=$(resim publish ".")
export pkg=$(echo $result | awk -F ": " '{print $2}')
# export pkg="package_sim1qrjd0mype69u6dpl7gqftwrzqj0efdxklu2fqdff47zq64ja7d"

result=$(resim call-function $pkg PriceOracle "new"  $usdt 8.88333 $usdc 8.88333)
export oracle=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')

result=$(resim call-function $pkg DefInterestModel "new")
export def_interest_model=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')

result=$(resim call-function $pkg StableInterestModel "new")
export stable_interest_model=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')



result=$(resim call-function $pkg LendingFactory "instantiate_lending_factory" $oracle)
export component=$(echo $result | grep "Component: "| awk -F "Component: " '{print $2}' | awk -F " " '{print $1}')
export lending_badge=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}' | awk -F " " '{print $1}')
export cdp=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $4}')


xrd="resource_sim1qyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqs6d89k"
result=$(resim call-method $component 'new_pool' $xrd 0.6 0.7 0.07 0.25 $def_interest_model --proofs "$lending_badge:1")
export dx_xrd=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')
result=$(resim call-method $component 'new_pool' $usdc 0.85 0.87 0.02 0.1 $stable_interest_model --proofs "$lending_badge:1")
export dx_usdc=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')
result=$(resim call-method $component 'new_pool' $usdt 0 0 0 0.1 $stable_interest_model --proofs "$lending_badge:1")
export dx_usdt=$(echo $result | grep "Resource: " | awk -F "Resource: " '{if (NR==1) print $2}')


resim set-default-account $p1 $p1_priv $p1_badge
resim call-method $component 'supply' $xrd:5000

resim set-default-account $p2 $p2_priv $p2_badge
resim call-method $component 'supply' $xrd:5000

resim set-default-account $p2 $p2_priv $p2_badge
resim call-method $component 'supply' $usdt:360

resim set-default-account $p3 $p3_priv $p3_badge
resim call-method $component 'supply' $usdc:360

resim set-default-account $p1 $p1_priv $p1_badge
resim call-method $component 'borrow' $dx_xrd:5000 $usdt 180
resim call-method $component 'borrow' $dx_xrd:5000 $usdc 180

resim call-method $component 'get_interest_rate' $usdt






