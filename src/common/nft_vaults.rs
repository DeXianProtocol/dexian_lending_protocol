use scrypto::prelude::*;

/**
 * A non-fungible vault that can store non-fungible tokens.
 */
#[derive(ScryptoSbor)]
pub struct NonFungibleVaults {
    vaults: KeyValueStore<ResourceAddress, NonFungibleVault>,
}

impl NonFungibleVaults {
    pub fn new<F>(create_fn: F) -> Self 
    where
        F: Fn() -> KeyValueStore<ResourceAddress, NonFungibleVault>,
    {
        Self { 
            vaults: create_fn(),
        }
    }

    /**
     * Retrieve the specific NFT from the vault.
     */
    pub fn take_nft(&mut self, nft_id: &NonFungibleGlobalId) -> Option<NonFungibleBucket> {
        if let Some(mut vault) = self.vaults.get_mut(&nft_id.resource_address()) {
            Some(vault.take_non_fungible(nft_id.local_id()))
        } else {
            Option::None
        }
    }

    pub fn take_nft_batch(&mut self, nft_ids: Vec<NonFungibleGlobalId>) -> Vec<NonFungibleBucket> {
        let mut buckets_map: HashMap<ResourceAddress, NonFungibleBucket> = HashMap::new();
        for nft_id in nft_ids {
            if let Some(bucket) = self.take_nft(&nft_id) {
                let resource_address = nft_id.resource_address();
                if let Some(existing_bucket) = buckets_map.get_mut(&resource_address) {
                    existing_bucket.put(bucket);
                } else {
                    buckets_map.insert(resource_address, bucket);
                }
            }
        }
        buckets_map.into_values().collect()
    }


    pub fn put(&mut self, nft_bucket: NonFungibleBucket) {
        let resource = nft_bucket.resource_address();
        if self.vaults.get(&resource).is_some() {
            let mut vault = self.vaults.get_mut(&resource).unwrap();
            vault.put(nft_bucket);
        } else {
            self.vaults.insert(resource, NonFungibleVault::with_bucket(nft_bucket));
        }
    }

    pub fn put_batch(&mut self, nft_list: Vec<NonFungibleBucket>) {
        for nft in nft_list {
            self.put(nft);
        }
    }

}
