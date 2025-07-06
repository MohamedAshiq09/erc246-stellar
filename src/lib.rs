#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror, token, Address, Env, String, Symbol,
    symbol_short, Vec, Map
};

#[contracttype]
pub enum DataKey {
    Asset,
    Name,
    Symbol,
    Decimals,
    TotalSupply,
    Balance(Address),
    Allowance(Address, Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    ZeroAssets = 1,
    ZeroShares = 2,
    InsufficientBalance = 3,
    InsufficientAllowance = 4,
    InvalidAddress = 5,
}

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn initialize(
        env: Env,
        asset: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Asset) {
            return Err(Error::InvalidAddress);
        }
        
        env.storage().instance().set(&DataKey::Asset, &asset);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        
        Ok(())
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&DataKey::Name).unwrap_or(String::from_str(&env, "Vault"))
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&DataKey::Symbol).unwrap_or(String::from_str(&env, "VAULT"))
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Decimals).unwrap_or(18)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0)
    }

    pub fn balance_of(env: Env, account: Address) -> i128 {
        env.storage().instance().get(&DataKey::Balance(account)).unwrap_or(0)
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        env.storage().instance().get(&DataKey::Allowance(owner, spender)).unwrap_or(0)
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) -> Result<bool, Error> {
        from.require_auth();
        Self::transfer_internal(&env, from, to, amount)?;
        Ok(true)
    }

    pub fn approve(env: Env, from: Address, spender: Address, amount: i128) -> bool {
        from.require_auth();
        env.storage().instance().set(&DataKey::Allowance(from.clone(), spender.clone()), &amount);
        
        env.events().publish(
            (symbol_short!("approve"), from, spender),
            amount
        );
        true
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) -> Result<bool, Error> {
        spender.require_auth();
        
        let allowance = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowance < amount {
            return Err(Error::InsufficientAllowance);
        }
        
        if allowance != i128::MAX {
            env.storage().instance().set(
                &DataKey::Allowance(from.clone(), spender),
                &(allowance - amount)
            );
        }
        
        Self::transfer_internal(&env, from, to, amount)?;
        Ok(true)
    }

    // ERC4626 Vault Interface
    pub fn asset(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Asset).unwrap()
    }

    pub fn total_assets(env: Env) -> i128 {
        let asset_address = Self::asset(env.clone());
        let asset_client = token::Client::new(&env, &asset_address);
        asset_client.balance(&env.current_contract_address())
    }

    pub fn convert_to_shares(env: Env, assets: i128) -> i128 {
        Self::convert_to_shares_internal(&env, assets, false)
    }

    pub fn convert_to_assets(env: Env, shares: i128) -> i128 {
        Self::convert_to_assets_internal(&env, shares, false)
    }

    pub fn max_deposit(_env: Env, _receiver: Address) -> i128 {
        i128::MAX
    }

    pub fn max_mint(_env: Env, _receiver: Address) -> i128 {
        i128::MAX
    }

    pub fn max_withdraw(env: Env, owner: Address) -> i128 {
        let shares = Self::balance_of(env.clone(), owner);
        Self::convert_to_assets_internal(&env, shares, false)
    }

    pub fn max_redeem(env: Env, owner: Address) -> i128 {
        Self::balance_of(env, owner)
    }

    pub fn preview_deposit(env: Env, assets: i128) -> i128 {
        Self::convert_to_shares_internal(&env, assets, false)
    }

    pub fn preview_mint(env: Env, shares: i128) -> i128 {
        Self::convert_to_assets_internal(&env, shares, true)
    }

    pub fn preview_withdraw(env: Env, assets: i128) -> i128 {
        Self::convert_to_shares_internal(&env, assets, true)
    }

    pub fn preview_redeem(env: Env, shares: i128) -> i128 {
        Self::convert_to_assets_internal(&env, shares, false)
    }

    pub fn deposit(env: Env, assets: i128, receiver: Address) -> Result<i128, Error> {
        let caller = env.current_contract_address();
        caller.require_auth();
        
        if assets <= 0 {
            return Err(Error::ZeroAssets);
        }
        
        let shares = Self::preview_deposit(env.clone(), assets);
        if shares <= 0 {
            return Err(Error::ZeroShares);
        }
        
        let asset_address = Self::asset(env.clone());
        let asset_client = token::Client::new(&env, &asset_address);
        asset_client.transfer(&caller, &env.current_contract_address(), &assets);
        
        Self::mint_internal(&env, receiver.clone(), shares);
        
        env.events().publish(
            (symbol_short!("deposit"), caller, receiver),
            (assets, shares)
        );
        
        Ok(shares)
    }

    pub fn mint(env: Env, shares: i128, receiver: Address) -> Result<i128, Error> {
        let caller = env.current_contract_address();
        caller.require_auth();
        
        if shares <= 0 {
            return Err(Error::ZeroShares);
        }
        
        let assets = Self::preview_mint(env.clone(), shares);
        if assets <= 0 {
            return Err(Error::ZeroAssets);
        }
        
        let asset_address = Self::asset(env.clone());
        let asset_client = token::Client::new(&env, &asset_address);
        asset_client.transfer(&caller, &env.current_contract_address(), &assets);
        
        Self::mint_internal(&env, receiver.clone(), shares);
        
        env.events().publish(
            (symbol_short!("deposit"), caller, receiver),
            (assets, shares)
        );
        
        Ok(assets)
    }

    pub fn withdraw(env: Env, assets: i128, receiver: Address, owner: Address) -> Result<i128, Error> {
        let caller = env.current_contract_address();
        caller.require_auth();
        
        if assets <= 0 {
            return Err(Error::ZeroAssets);
        }
        
        let shares = Self::preview_withdraw(env.clone(), assets);
        if shares <= 0 {
            return Err(Error::ZeroShares);
        }
        
        if caller != owner {
            let allowance = Self::allowance(env.clone(), owner.clone(), caller.clone());
            if allowance < shares {
                return Err(Error::InsufficientAllowance);
            }
            if allowance != i128::MAX {
                env.storage().instance().set(
                    &DataKey::Allowance(owner.clone(), caller.clone()),
                    &(allowance - shares)
                );
            }
        }
        
        Self::burn_internal(&env, owner.clone(), shares)?;
        
        let asset_address = Self::asset(env.clone());
        let asset_client = token::Client::new(&env, &asset_address);
        asset_client.transfer(&env.current_contract_address(), &receiver, &assets);
        
        env.events().publish(
            (symbol_short!("withdraw"), caller, receiver, owner),
            (assets, shares)
        );
        
        Ok(shares)
    }

    pub fn redeem(env: Env, shares: i128, receiver: Address, owner: Address) -> Result<i128, Error> {
        let caller = env.current_contract_address();
        caller.require_auth();
        
        if shares <= 0 {
            return Err(Error::ZeroShares);
        }
        
        let assets = Self::preview_redeem(env.clone(), shares);
        if assets <= 0 {
            return Err(Error::ZeroAssets);
        }
        
        if caller != owner {
            let allowance = Self::allowance(env.clone(), owner.clone(), caller.clone());
            if allowance < shares {
                return Err(Error::InsufficientAllowance);
            }
            if allowance != i128::MAX {
                env.storage().instance().set(
                    &DataKey::Allowance(owner.clone(), caller.clone()),
                    &(allowance - shares)
                );
            }
        }
        
        Self::burn_internal(&env, owner.clone(), shares)?;
        
        let asset_address = Self::asset(env.clone());
        let asset_client = token::Client::new(&env, &asset_address);
        asset_client.transfer(&env.current_contract_address(), &receiver, &assets);
        
        env.events().publish(
            (symbol_short!("withdraw"), caller, receiver, owner),
            (assets, shares)
        );
        
        Ok(assets)
    }

    fn transfer_internal(env: &Env, from: Address, to: Address, amount: i128) -> Result<(), Error> {
        let from_balance = Self::balance_of(env.clone(), from.clone());
        if from_balance < amount {
            return Err(Error::InsufficientBalance);
        }
        
        env.storage().instance().set(&DataKey::Balance(from.clone()), &(from_balance - amount));
        
        let to_balance = Self::balance_of(env.clone(), to.clone());
        env.storage().instance().set(&DataKey::Balance(to.clone()), &(to_balance + amount));
        
        env.events().publish(
            (symbol_short!("transfer"), from, to),
            amount
        );
        
        Ok(())
    }

    fn mint_internal(env: &Env, account: Address, amount: i128) {
        let balance = Self::balance_of(env.clone(), account.clone());
        env.storage().instance().set(&DataKey::Balance(account.clone()), &(balance + amount));
        
        let total_supply = Self::total_supply(env.clone());
        env.storage().instance().set(&DataKey::TotalSupply, &(total_supply + amount));
        
        env.events().publish(
            (symbol_short!("mint"), account),
            amount
        );
    }

    fn burn_internal(env: &Env, account: Address, amount: i128) -> Result<(), Error> {
        let balance = Self::balance_of(env.clone(), account.clone());
        if balance < amount {
            return Err(Error::InsufficientBalance);
        }
        
        env.storage().instance().set(&DataKey::Balance(account.clone()), &(balance - amount));
        
        let total_supply = Self::total_supply(env.clone());
        env.storage().instance().set(&DataKey::TotalSupply, &(total_supply - amount));
        
        env.events().publish(
            (symbol_short!("burn"), account),
            amount
        );
        
        Ok(())
    }

    fn convert_to_shares_internal(env: &Env, assets: i128, round_up: bool) -> i128 {
        let supply = Self::total_supply(env.clone());
        let total = Self::total_assets(env.clone());
        
        if supply == 0 || total == 0 {
            return assets;
        }
        
        let result = (assets * supply) / total;
        if round_up && (assets * supply) % total > 0 {
            result + 1
        } else {
            result
        }
    }

    fn convert_to_assets_internal(env: &Env, shares: i128, round_up: bool) -> i128 {
        let supply = Self::total_supply(env.clone());
        let total = Self::total_assets(env.clone());
        
        if supply == 0 || total == 0 {
            return shares;
        }
        
        let result = (shares * total) / supply;
        if round_up && (shares * total) % supply > 0 {
            result + 1
        } else {
            result
        }
    }
}