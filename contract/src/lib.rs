use external::{ext_self, GAS_FOR_AFTER_CLAIM};
use jar::{Jar, JarIndex};
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::borsh::maybestd::collections::HashSet;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, UnorderedSet, Vector};
use near_sdk::json_types::U128;
use near_sdk::{
    env, near_bindgen, AccountId, Balance, BorshStorageKey, Gas, PanicOnDefault, PromiseOrValue,
};
use product::{Product, ProductApi, ProductId};

use crate::external::GAS_FOR_AFTER_WITHDRAW;

mod assert;
mod common;
mod external;
mod ft_receiver;
mod internal;
mod jar;
mod product;

// TODO
// 1. view get_principal
// 2. view get_interest
// 3. create deposit by transfer
// 4. claim all the interest

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub token_account_id: AccountId,
    pub admin_allowlist: UnorderedSet<AccountId>,

    pub products: UnorderedMap<ProductId, Product>,

    pub jars: Vector<Jar>,
    pub account_jars: LookupMap<AccountId, HashSet<JarIndex>>,
}

#[derive(BorshStorageKey, BorshSerialize)]
pub(crate) enum StorageKey {
    Administrators,
    Products,
    Jars,
    AccountJars,
}

pub trait ContractApi {
    fn get_principal(&self, account_id: AccountId) -> Balance;
    fn get_interest(&self, account_id: AccountId) -> Balance;
    fn claim(&mut self) -> PromiseOrValue<Balance>;
    fn withdraw(&mut self, jar_id: JarIndex) -> PromiseOrValue<Balance>;
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn init(token_account_id: AccountId, admin_allowlist: Vec<AccountId>) -> Self {
        let mut admin_allowlist_set = UnorderedSet::new(StorageKey::Administrators);
        admin_allowlist_set.extend(admin_allowlist.clone().into_iter().map(|item| item.into()));

        Self {
            token_account_id,
            admin_allowlist: admin_allowlist_set,
            products: UnorderedMap::new(StorageKey::Products),
            jars: Vector::new(StorageKey::Jars),
            account_jars: LookupMap::new(StorageKey::AccountJars),
        }
    }

    #[private]
    pub fn create_jar(
        &mut self,
        account_id: AccountId,
        product_id: ProductId,
        amount: Balance,
    ) -> Jar {
        assert!(
            self.products.get(&product_id).is_some(),
            "Product doesn't exist"
        );

        let index = self.jars.len() as JarIndex;
        let now = env::block_timestamp_ms();
        let jar = Jar::create(index, account_id.clone(), product_id, amount, now);

        self.save_jar(&account_id, &jar);

        return jar;
    }
}

#[near_bindgen]
impl ContractApi for Contract {
    fn get_principal(&self, account_id: AccountId) -> Balance {
        let mut result: Balance = 0;
        let jar_ids = self
            .account_jars
            .get(&account_id)
            .clone()
            .expect("Account doesn't have jars")
            .clone();

        let jar_ids_iter = jar_ids.iter();
        for i in jar_ids_iter {
            let jar = self
                .jars
                .get(*i as _)
                .expect(format!("Jar on index {} doesn't exist", i).as_ref());
            result += jar.principal;
        }

        result
    }

    fn get_interest(&self, account_id: AccountId) -> Balance {
        let mut result: Balance = 0;
        let jar_ids = self
            .account_jars
            .get(&account_id)
            .clone()
            .expect("Account doesn't have jars")
            .clone();
        let now = env::block_timestamp_ms();

        let jar_ids_iter = jar_ids.iter();
        for i in jar_ids_iter {
            let jar = self
                .jars
                .get(*i as _)
                .expect(format!("Jar on index {} doesn't exist", i).as_ref());

            let product = self
                .products
                .get(&jar.product_id)
                .expect("Product doesn't exist");

            result += jar.get_interest(&product, now);
        }

        result
    }

    fn claim(&mut self) -> PromiseOrValue<Balance> {
        let mut interest: Balance = 0;
        let account_id = env::predecessor_account_id().clone();
        let jar_ids = self
            .account_jars
            .get(&account_id)
            .clone()
            .expect("Account doesn't have jars")
            .clone();
        let now = env::block_timestamp_ms();

        let jar_ids_iter = jar_ids.iter();
        for i in jar_ids_iter {
            let jar = self
                .jars
                .get(*i as _)
                .expect(format!("Jar on index {} doesn't exist", i).as_ref());

            let product = self
                .products
                .get(&jar.product_id)
                .expect("Product doesn't exist");

            interest += jar.get_interest(&product, now);

            let updated_jar = jar.claimed(interest, now);
            self.jars.replace(*i as _, &updated_jar);
        }

        if interest > 0 {
            ext_ft_core::ext(self.token_account_id.clone())
                .with_attached_deposit(1)
                .ft_transfer(account_id.clone(), U128::from(interest.clone()), None)
                .then(
                    ext_self::ext(env::current_account_id())
                        .with_static_gas(Gas::from(GAS_FOR_AFTER_CLAIM))
                        .after_claim(account_id, interest.clone()),
                )
                .into()
        } else {
            PromiseOrValue::Value(0)
        }
    }

    fn withdraw(&mut self, jar_index: JarIndex) -> PromiseOrValue<Balance> {
        let jar = self.jars.get(jar_index as _).expect("Jar doesn't exist");
        let account_id = env::predecessor_account_id();

        assert_eq!(
            jar.account_id.clone(),
            account_id.clone(),
            "Account doesn't own this jar"
        );

        assert!(jar.principal > 0, "Jar is empty");

        ext_ft_core::ext(self.token_account_id.clone())
            .with_attached_deposit(1)
            .ft_transfer(account_id.clone(), U128::from(jar.principal), None)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from(GAS_FOR_AFTER_WITHDRAW))
                    .after_withdrow(account_id, jar.principal),
            )
            .into()
    }
}

#[near_bindgen]
impl ProductApi for Contract {
    fn register_product(&mut self, product: Product) {
        self.assert_admin();

        self.products.insert(&product.id, &product);
    }

    fn get_products(&self) -> Vec<Product> {
        self.products.values_as_vector().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::{
        test_utils::{accounts, VMContextBuilder},
        testing_env,
    };

    use crate::common::UDecimal;

    use super::*;

    fn get_product() -> Product {
        Product {
            id: "product".to_string(),
            lockup_term: 365 * 60 * 60 * 1000 * 1000,
            maturity_term: Some(365 * 60 * 60 * 1000 * 1000),
            notice_term: None,
            is_refillable: false,
            apy: UDecimal {
                significand: 1,
                exponent: 1,
            },
            cap: 100,
            is_restakable: false,
            withdrowal_fee: None,
            is_public: true,
        }
    }

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id.clone())
            .block_timestamp(0);

        builder
    }

    #[test]
    fn add_product_to_list_by_admin() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        contract.register_product(get_product());

        let products = contract.get_products();
        assert_eq!(products.len(), 1);
        assert_eq!(products.first().unwrap().id, "product".to_string());
    }

    #[test]
    #[should_panic(expected = "Can be performed only by admin")]
    fn add_product_to_list_by_not_admin() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(1)],
        );

        contract.register_product(get_product());
    }

    #[test]
    #[should_panic(expected = "Account doesn't have jars")]
    fn get_principle_with_no_jars() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(1)],
        );

        contract.get_principal(accounts(0));
    }

    #[test]
    fn get_principal_with_single_jar() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);

        testing_env!(get_context(accounts(1)).build());

        let principal = contract.get_principal(accounts(1));
        assert_eq!(principal, 100);
    }

    #[test]
    fn get_principal_with_multiple_jars() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);
        contract.create_jar(accounts(1), product.clone().id, 200);
        contract.create_jar(accounts(1), product.clone().id, 400);

        testing_env!(get_context(accounts(1)).build());

        let principal = contract.get_principal(accounts(1));
        assert_eq!(principal, 700);
    }

    #[test]
    #[should_panic(expected = "Account doesn't have jars")]
    fn get_total_interest_with_no_jars() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        contract.get_interest(accounts(0));
    }

    #[test]
    fn get_total_interest_with_single_jar_after_half_term() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);

        testing_env!(get_context(accounts(1))
            .block_timestamp(183 * 24 * 60 * 60 * u64::pow(10, 9))
            .build());

        let interest = contract.get_interest(accounts(1));
        assert_eq!(interest, 5);
    }

    #[test]
    fn get_total_interest_with_single_jar_on_maturity() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);

        testing_env!(get_context(accounts(1))
            .block_timestamp(366 * 24 * 60 * 60 * u64::pow(10, 9))
            .build());

        let interest = contract.get_interest(accounts(1));
        assert_eq!(interest, 10);
    }

    #[test]
    fn get_total_interest_with_single_jar_after_maturity() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);

        testing_env!(get_context(accounts(1))
            .block_timestamp(400 * 24 * 60 * 60 * u64::pow(10, 9))
            .build());

        let interest = contract.get_interest(accounts(1));
        assert_eq!(interest, 10);
    }

    #[test]
    fn get_total_interest_with_single_jar_after_claim_on_half_term_and_maturity() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::init(
            AccountId::new_unchecked("token".to_string()),
            vec![accounts(0)],
        );

        let product = get_product();

        contract.register_product(product.clone());
        contract.create_jar(accounts(1), product.clone().id, 100);

        testing_env!(get_context(accounts(1))
            .block_timestamp(183 * 24 * 60 * 60 * u64::pow(10, 9))
            .build());

        contract.claim();

        testing_env!(get_context(accounts(1))
            .block_timestamp(366 * 24 * 60 * 60 * u64::pow(10, 9))
            .build());

        let interest = contract.get_interest(accounts(1));
        assert_eq!(interest, 5);
    }

    //    #[test]
    //    fn get_half_of_interest_when_claim_on_half_term() {
    //        let context = get_context(accounts(0));
    //        testing_env!(context.build());
    //        let mut contract = Contract::init(
    //            AccountId::new_unchecked("token".to_string()),
    //            vec![accounts(0)],
    //        );
    //
    //        let product = get_product();
    //
    //        contract.register_product(product.clone());
    //        contract.create_jar(accounts(1), product.clone().id, 100);
    //
    //        testing_env!(get_context(accounts(1))
    //            .block_timestamp(183 * 24 * 60 * 60 * u64::pow(10, 9))
    //            .build());
    //
    //        let claim_promise = contract.claim();
    //
    //        if let PromiseOrValue::Promise(promise) = contract.claim() {
    //        }
    //        let interest = contract.claim();
    //
    //        assert_eq!(interest, 5);
    //    }
    //
    //    #[test]
    //    fn get_total_interest_when_claim_on_maturity() {
    //        let context = get_context(accounts(0));
    //        testing_env!(context.build());
    //        let mut contract = Contract::init(
    //            AccountId::new_unchecked("token".to_string()),
    //            vec![accounts(0)],
    //        );
    //
    //        let product = get_product();
    //
    //        contract.register_product(product.clone());
    //        contract.create_jar(accounts(1), product.clone().id, 100);
    //
    //        testing_env!(get_context(accounts(1))
    //            .block_timestamp(366 * 24 * 60 * 60 * u64::pow(10, 9))
    //            .build());
    //
    //        let interest = contract.claim();
    //
    //        assert_eq!(interest, 10);
    //    }
}
