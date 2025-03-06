use scrypto::prelude::*;

/**
 * A vault that can store fungible tokens.
 */
#[derive(ScryptoSbor)]
pub struct Vaults {
    vaults: KeyValueStore<ResourceAddress, FungibleVault>,
}

impl Vaults {
    pub fn new<F>(create_fn: F) -> Self 
    where
        F: Fn() -> KeyValueStore<ResourceAddress, FungibleVault>,
    {
        Self { 
            vaults: create_fn(),
        }
    }

    pub fn amount(&self, resource: &ResourceAddress) -> Decimal {
        if let Some(vault) = self.vaults.get(resource) {
            vault.amount()
        } else {
            dec!(0)
        }
    }

    pub fn amounts(&self, resources: Vec<ResourceAddress>) -> HashMap<ResourceAddress, Decimal> {
        resources.into_iter().map(|resource| (resource, self.amount(&resource))).collect()
    }

    pub fn put(&mut self, tokens: FungibleBucket) {
        let resource = tokens.resource_address();
        if self.vaults.get(&resource).is_some() {
            let mut vault = self.vaults.get_mut(&resource).unwrap();
            vault.put(tokens);
        } else {
            self.vaults.insert(resource, FungibleVault::with_bucket(tokens));
        }
    }

    pub fn put_batch(&mut self, tokens: Vec<FungibleBucket>) {
        for token in tokens {
            self.put(token);
        }
    }

    pub fn take(&mut self, resource: &ResourceAddress, amount: Decimal) -> FungibleBucket {
        if self.vaults.get(&resource).is_none() {
            self.vaults.insert(*resource, FungibleVault::new(*resource));
        }
        let mut vault = self.vaults.get_mut(&resource).unwrap();
        vault.take(amount)
    }

    pub fn take_batch(&mut self, claims: Vec<(ResourceAddress, Decimal)>) -> Vec<FungibleBucket> {
        claims
            .into_iter()
            .map(|(resource, amount)| self.take(&resource, amount))
            .collect()
    }

    pub fn take_advanced(&mut self, resource: &ResourceAddress, amount: Decimal, withdraw_strategy: WithdrawStrategy) -> FungibleBucket {
        if self.vaults.get(&resource).is_none() {
            self.vaults.insert(*resource, FungibleVault::new(*resource));
        }
        let mut vault = self.vaults.get_mut(&resource).unwrap();
        vault.take_advanced(amount, withdraw_strategy)
    }

    pub fn take_advanced_batch(&mut self, claims: Vec<(ResourceAddress, Decimal)>, withdraw_strategy: WithdrawStrategy) -> Vec<FungibleBucket> {
        claims
            .into_iter()
            .map(|(resource, amount)| self.take_advanced(&resource, amount, withdraw_strategy))
            .collect()
    }
}
