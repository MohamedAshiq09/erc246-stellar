#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    Address, Env, String, Symbol,
};

// Mock token contract for testing
mod token {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, panic_with_error};

    #[contracttype]
    pub enum DataKey {
        Balance(Address),
        TotalSupply,
        Allowance(Address, Address),
    }

    #[contracttype]
    pub enum Error {
        InsufficientBalance = 1,
        InsufficientAllowance = 2,
    }

    #[contract]
    pub struct MockToken;

    #[contractimpl]
    impl MockToken {
        pub fn initialize(env: Env, total_supply: i128) {
            env.storage().instance().set(&DataKey::TotalSupply, &total_supply);
        }

        pub fn balance(env: Env, account: Address) -> i128 {
            env.storage().instance().get(&DataKey::Balance(account)).unwrap_or(0)
        }

        pub fn total_supply(env: Env) -> i128 {
            env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            
            let from_balance = Self::balance(env.clone(), from.clone());
            if from_balance < amount {
                panic_with_error!(&env, Error::InsufficientBalance);
            }
            
            env.storage().instance().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
            
            let to_balance = Self::balance(env.clone(), to.clone());
            env.storage().instance().set(&DataKey::Balance(to.clone()), &(to_balance + amount));
        }

        pub fn mint(env: Env, to: Address, amount: i128) {
            let balance = Self::balance(env.clone(), to.clone());
            env.storage().instance().set(&DataKey::Balance(to.clone()), &(balance + amount));
            
            let total_supply = Self::total_supply(env.clone());
            env.storage().instance().set(&DataKey::TotalSupply, &(total_supply + amount));
        }
    }
}

// Test helper struct
struct TestSetup {
    env: Env,
    vault_id: Address,
    token_id: Address,
    user: Address,
    user2: Address,
}

impl TestSetup {
    fn new() -> Self {
        let env = Env::default();
        let vault_id = env.register_contract(None, VaultContract);
        let token_id = env.register_contract(None, token::MockToken);
        let user = Address::generate(&env);
        let user2 = Address::generate(&env);

        Self {
            env,
            vault_id,
            token_id,
            user,
            user2,
        }
    }

    fn initialize_vault(&self, name: &str, symbol: &str, decimals: u32) {
        let client = VaultContractClient::new(&self.env, &self.vault_id);
        client.initialize(
            &self.token_id,
            &String::from_str(&self.env, name),
            &String::from_str(&self.env, symbol),
            &decimals,
        );
    }

    fn initialize_token(&self, initial_supply: i128) {
        let client = token::MockTokenClient::new(&self.env, &self.token_id);
        client.initialize(&initial_supply);
    }

    fn mint_tokens(&self, to: &Address, amount: i128) {
        let client = token::MockTokenClient::new(&self.env, &self.token_id);
        client.mint(to, &amount);
    }
}

#[test]
fn test_initialize() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    
    assert_eq!(client.name(), String::from_str(&setup.env, "Test Vault"));
    assert_eq!(client.symbol(), String::from_str(&setup.env, "TVAULT"));
    assert_eq!(client.decimals(), 18);
    assert_eq!(client.total_supply(), 0);
    assert_eq!(client.asset(), setup.token_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_initialize_twice() {
    let setup = TestSetup::new();
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    // Should panic when trying to initialize again
    setup.initialize_vault("Test Vault 2", "TVAULT2", 18);
}

#[test]
fn test_erc20_functionality() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    // Test initial balances
    assert_eq!(client.balance_of(&setup.user), 0);
    assert_eq!(client.balance_of(&setup.user2), 0);
    
    // Test deposit to get some vault tokens
    setup.env.mock_all_auths();
    let shares = client.deposit(&100, &setup.user);
    
    assert!(shares > 0);
    assert_eq!(client.balance_of(&setup.user), shares);
    assert_eq!(client.total_supply(), shares);
    
    // Test transfer
    setup.env.mock_all_auths();
    assert!(client.transfer(&setup.user, &setup.user2, &50));
    assert_eq!(client.balance_of(&setup.user), shares - 50);
    assert_eq!(client.balance_of(&setup.user2), 50);
    
    // Test approve and allowance
    setup.env.mock_all_auths();
    assert!(client.approve(&setup.user, &setup.user2, &25));
    assert_eq!(client.allowance(&setup.user, &setup.user2), 25);
    
    // Test transfer_from
    setup.env.mock_all_auths();
    assert!(client.transfer_from(&setup.user2, &setup.user, &setup.user2, &25));
    assert_eq!(client.balance_of(&setup.user), shares - 75);
    assert_eq!(client.balance_of(&setup.user2), 75);
    assert_eq!(client.allowance(&setup.user, &setup.user2), 0);
}

#[test]
fn test_vault_deposit() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // First deposit - should get 1:1 ratio
    let shares = client.deposit(&100, &setup.user);
    assert_eq!(shares, 100);
    assert_eq!(client.balance_of(&setup.user), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.total_assets(), 100);
    
    // Second deposit - should still get 1:1 ratio
    let shares2 = client.deposit(&50, &setup.user);
    assert_eq!(shares2, 50);
    assert_eq!(client.balance_of(&setup.user), 150);
    assert_eq!(client.total_supply(), 150);
    assert_eq!(client.total_assets(), 150);
}

#[test]
fn test_vault_mint() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Mint 100 shares
    let assets = client.mint(&100, &setup.user);
    assert_eq!(assets, 100); // 1:1 ratio initially
    assert_eq!(client.balance_of(&setup.user), 100);
    assert_eq!(client.total_supply(), 100);
    assert_eq!(client.total_assets(), 100);
}

#[test]
fn test_vault_withdraw() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Deposit first
    client.deposit(&200, &setup.user);
    
    // Withdraw 50 assets
    let shares_burned = client.withdraw(&50, &setup.user2, &setup.user);
    assert_eq!(shares_burned, 50); // 1:1 ratio
    assert_eq!(client.balance_of(&setup.user), 150);
    assert_eq!(client.total_supply(), 150);
    assert_eq!(client.total_assets(), 150);
}

#[test]
fn test_vault_redeem() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Deposit first
    client.deposit(&200, &setup.user);
    
    // Redeem 50 shares
    let assets_received = client.redeem(&50, &setup.user2, &setup.user);
    assert_eq!(assets_received, 50); // 1:1 ratio
    assert_eq!(client.balance_of(&setup.user), 150);
    assert_eq!(client.total_supply(), 150);
    assert_eq!(client.total_assets(), 150);
}

#[test]
fn test_conversion_functions() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Initially, conversion should be 1:1
    assert_eq!(client.convert_to_shares(&100), 100);
    assert_eq!(client.convert_to_assets(&100), 100);
    
    // After deposit, should still be 1:1
    client.deposit(&200, &setup.user);
    assert_eq!(client.convert_to_shares(&100), 100);
    assert_eq!(client.convert_to_assets(&100), 100);
}

#[test]
fn test_preview_functions() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Test preview functions before any deposits
    assert_eq!(client.preview_deposit(&100), 100);
    assert_eq!(client.preview_mint(&100), 100);
    assert_eq!(client.preview_withdraw(&100), 100);
    assert_eq!(client.preview_redeem(&100), 100);
    
    // After deposit, previews should still work
    client.deposit(&200, &setup.user);
    assert_eq!(client.preview_deposit(&100), 100);
    assert_eq!(client.preview_mint(&100), 100);
    assert_eq!(client.preview_withdraw(&100), 100);
    assert_eq!(client.preview_redeem(&100), 100);
}

#[test]
fn test_max_functions() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Test max functions
    assert_eq!(client.max_deposit(&setup.user), i128::MAX);
    assert_eq!(client.max_mint(&setup.user), i128::MAX);
    assert_eq!(client.max_withdraw(&setup.user), 0); // No shares yet
    assert_eq!(client.max_redeem(&setup.user), 0); // No shares yet
    
    // After deposit
    client.deposit(&200, &setup.user);
    assert_eq!(client.max_withdraw(&setup.user), 200);
    assert_eq!(client.max_redeem(&setup.user), 200);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_deposit_zero_assets() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    client.deposit(&0, &setup.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_mint_zero_shares() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    client.mint(&0, &setup.user);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_transfer_insufficient_balance() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Deposit some tokens
    client.deposit(&100, &setup.user);
    
    // Try to transfer more than balance
    client.transfer(&setup.user, &setup.user2, &200);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_transfer_from_insufficient_allowance() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Deposit some tokens
    client.deposit(&100, &setup.user);
    
    // Approve less than transfer amount
    client.approve(&setup.user, &setup.user2, &50);
    
    // Try to transfer more than allowance
    client.transfer_from(&setup.user2, &setup.user, &setup.user2, &100);
}

#[test]
fn test_events() {
    let setup = TestSetup::new();
    let client = VaultContractClient::new(&setup.env, &setup.vault_id);
    
    setup.initialize_vault("Test Vault", "TVAULT", 18);
    setup.initialize_token(1_000_000);
    setup.mint_tokens(&setup.user, 1000);
    
    setup.env.mock_all_auths();
    
    // Test deposit event
    client.deposit(&100, &setup.user);
    
    let events = setup.env.events().all();
    assert!(events.len() > 0);
    
    // Test transfer event
    client.transfer(&setup.user, &setup.user2, &50);
    
    let events = setup.env.events().all();
    assert!(events.len() > 1);
}