use alloy::primitives::{Address, U256, keccak256};
use alloy::sol_types::{SolCall, SolValue};
use evm_knowledge::{
    environment_deployment::{deploy_lock_contract, spin_up_anvil_instance},
    fetch_values
};
use revm::{
    Evm,
    DatabaseRef, db::CacheDB,
    primitives::{TxKind, ExecutionResult, Output},
};

use evm_knowledge::contract_bindings::gate_lock::GateLock;


#[tokio::main]
async fn main() -> eyre::Result<()> {
    let controls = spin_up_anvil_instance().await?;
    let payload = fetch_values();

    let deploy_address = deploy_lock_contract(&controls, payload).await?;

    assert!(solve(deploy_address, controls).await?);
    Ok(())
}

/// Calculates the storage slot for a given key and mapping slot
/// Doc: https://docs.soliditylang.org/en/v0.8.26/internals/layout_in_storage.html#mappings-and-dynamic-arrays
/// The value corresponding to a mapping key k is located at keccak256(h(k) . p) where . is concatenation and h is a function that is applied to the key depending on its type:
/// 
/// for value types, h pads the value to 32 bytes in the same way as when storing the value in memory.
fn calculate_storage_slot(key: U256, mapping_slot: U256) -> U256 {
    // keccak256(abi.encode(key, mapping_slot))
    let mut data = [0u8; 64];
    data[0..32].copy_from_slice(&key.to_be_bytes::<32>());
    data[32..64].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(data).0)
}

/// Writes a storage value to the cache database
fn write_storage<DB: DatabaseRef>(
    cache_db: &mut CacheDB<DB>,
    address: Address,
    slot: U256,
    value: U256,
) -> eyre::Result<()> {
    cache_db
        .insert_account_storage(address, slot, value)
        .map_err(|_| eyre::eyre!("failed to write storage"))
}

/// Calls isSolved(uint256[] ids) on the contract using REVM
fn call_is_solved<DB: DatabaseRef>(
    cache_db: &mut CacheDB<DB>,
    contract_address: Address,
    keys: Vec<U256>,
) -> eyre::Result<bool> {
    // Encode the function call using the generated bindings
    let call = GateLock::isSolvedCall { ids: keys };
    let calldata = call.abi_encode();

    // Build and execute the EVM call
    let mut evm = Evm::builder()
        .with_db(cache_db)
        .modify_tx_env(|tx| {
            tx.transact_to = TxKind::Call(contract_address);
            tx.data = calldata.into();
        })
        .build();

    let result = evm.transact().map_err(|_| eyre::eyre!("EVM execution failed"))?;

    // Extract the return value
    match result.result {
        ExecutionResult::Success { output, .. } => {
            match output {
                Output::Call(bytes) => {
                    // Decode the bool return value
                    let decoded = bool::abi_decode(&bytes, true)
                        .map_err(|_| eyre::eyre!("failed to decode return value"))?;
                    Ok(decoded)
                }
                _ => Err(eyre::eyre!("unexpected output type")),
            }
        }
        ExecutionResult::Revert { output, .. } => {
            Err(eyre::eyre!("call reverted: {:?}", output))
        }
        ExecutionResult::Halt { reason, .. } => {
            Err(eyre::eyre!("call halted: {:?}", reason))
        }
    }
}

// your solution goes here.
async fn solve<DB: DatabaseRef>(contract_address: Address, db: DB) -> eyre::Result<bool> {
    // Wrap in CacheDB for write capability
    let mut cache_db = CacheDB::new(db);

    // Load the account information from the database and
    // test getting the balance and nonce to verify that the contract is loaded correctly
    let account = cache_db
        .basic_ref(contract_address)
        .map_err(|_| eyre::eyre!("failed to load account"))?
        .ok_or_else(|| eyre::eyre!("contract account not found"))?;
    
    println!("loaded cotract with account balance: {:?} and nonce: {:?}", account.balance, account.nonce);

    // The list of keys we have unlocked
    let mut keys = Vec::new();

    // The slot of the value map in the current contract is fixed as 2
    // as this is the third slot in the contract, after the _a and _b mappings
    // NB: this could change if the order of the state variables is modified
    let value_map_slot = U256::from(2);

    // The current key of the current node in the linked list
    // The starting key is always 0, as per the contract's constructor
    let mut current_key = U256::ZERO;
    
    loop {
        // Calculate storage slot for this mapping key
        let storage_slot = calculate_storage_slot(current_key, value_map_slot);
        
        // Read the packed Values struct
        let value = cache_db.storage_ref(contract_address, storage_slot).map_err(|_| eyre::eyre!("failed to read storage"))?;
        
        // If empty, we've reached the end of the linked list
        if value == U256::ZERO {
            break;
        }
        
        // Save the current key to the list of keys
        keys.push(current_key);
        
        // The first item in a storage slot is stored lower-order aligned (https://docs.soliditylang.org/en/v0.8.26/internals/layout_in_storage.html)
        // storage layout:
        //   0-63:     firstValue   (64 bits = 8 bytes)
        //   64-223:   secondValue  (160 bits = 20 bytes)  
        //   224-231:  is_unlocked  (8 bits = 1 byte)
        //   232-255:  unused padding

        // Parse firstValue (lowest 8 bytes) and secondValue (next 20 bytes)
        // extract the first 64 bits using a mask (firstValue)
        let first_value: u64 = (value & U256::from(u64::MAX)).to::<u64>();
        // bit shift 64 bits and use a mask to extract the next 160 bits (secondValue)
        let second_value: U256 = (value >> 64) & ((U256::from(1) << 160) - U256::from(1));
        
        // Set is_unlocked to true and write back
        let unlocked = value | (U256::from(1) << 224);  // bit 224 = byte 28
        write_storage(&mut cache_db, contract_address, storage_slot, unlocked)?;
        
        // Compute next key
        current_key = if first_value % 2 == 0 {
            U256::from(first_value)
        } else {
            second_value
        };
    }

    println!("found keys: {:?}", keys.len());

    // Call isSolved(uint256[] ids) on the contract and return the result
    let is_solved = call_is_solved(&mut cache_db, contract_address, keys)?;
    println!("isSolved: {:?}", is_solved);

    Ok(is_solved)
}
