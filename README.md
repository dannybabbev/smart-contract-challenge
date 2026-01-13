# Contract Integration Challenge
In this task, you will be using anvil (as a ETH backend) and REVM to do storage manipulations in order to get the 
smart contract to return a true value. You will need to make sure that you have foundry setup on your computer.
If you don't, go [here](https://book.getfoundry.sh/getting-started/installation) and install via your favourite method.

## Scope
### Setup
The environment given to you will have some problems while building. This is on purpose and is expected that you are able to fix and
get the rust environment working.

**Setup Requirments**
It is required that for our contract setup that we make sure that we always auto generate the bindings to our smart contracts.
This must be done with a Rust buildscript. Whenever there is a change, we want our buildscript to detect it and rebuild the contract bindings 
so that if there is any integration changes that would be breaking, it would be caught by tests.
The solution should have it auto-generate into a file, looking something along the lines of:
```rust
#[rustfmt::skip]
pub mod gate_lock {
    alloy::sol!(
        #[allow(missing_docs)]
        #[sol(rpc, abi)]
        #[derive(Debug, Default, PartialEq, Eq,Hash, serde::Serialize, serde::Deserialize)]
        GateLock,
        "path/to/GateLock.json"
    );
}
```

### Constraints 
1) No setup code can be modified. The only place you can add your code is in the `solve` function, and subsequent helper functions.
2) Must use REVM, in the solution.

### Goals
1) get codebase to compile by setting up a build script to automatically setup contract bindings
2) setup and adjust a REVM environment so we can bypass the locks that are set in the contract but are immutable by default
3) calling `cargo run` should exit without error

### Solution
The solution should be able to run as many times as we want given the random payloads and always have the contract function `isSolved` return true.
Your solution should go in /bin/solution.rs


# Solution steps performed and sources used
### Rust Build Script
The first step of the challenge is to set up the project and make it compile. 
1. Set up a rust build script running automatically on `cargo build` when relevant files or directories change (https://doc.rust-lang.org/cargo/reference/build-scripts.html)
2. Run `forge build` to ensure we are working with the latest version of the contracts
3. Generate the bindings file as per the section **Setup Requirements**

**AI Usage:** Cursor in Ask mode with Claude Opus 4.5 to help me generate the boilerplate for `build.rs`. I make changes to the directories it monitored, by adding more and a step to ensure that `forge build` runs on each build - ensuring the _"Whenever there is a change, we want our buildscript to detect it and rebuild the contract bindings so that if there is any integration changes that would be breaking, it would be caught by tests."_ requirement is met.

**Comment:** I tried using the official forge command https://getfoundry.sh/forge/reference/bind but I couldn't make it compile because of a mismatch between the used alloy version and the foundry generated types. I believe that it would work with the newest versions of alloy.

**Sources used:**
 - https://doc.rust-lang.org/cargo/reference/build-scripts.html
 - https://getfoundry.sh/forge/reference/bind

### Tracking the `valueMap` mapping:
The logic of the `valueMap` linked list is clear. We need to start from 0 and then trace the next key location using the same algorithm as in `constructor` of `GateLock.sol`. Getting the memory locations on the values of the `mapping` is documented clearly in https://docs.soliditylang.org/en/v0.8.26/internals/layout_in_storage.html as `keccak256(h(k) . p)`. The first step was writing `calculate_storage_slot`. After that, we need to deconstruct `firstValue`, `secondValue` and `unlocked` from the value we got from memory. Also, following the docs from Layout in Storage we can deduce the following layout:
```
0-63:     firstValue   (64 bits = 8 bytes)
64-223:   secondValue  (160 bits = 20 bytes)  
224-231:  is_unlocked  (8 bits = 1 byte)
232-255:  unused padding
```
Reading the storage slot is done following the documentation in https://docs.rs/revm/latest/revm/trait.DatabaseRef.html using the `storage_ref()` method.

**AI Usage:** Claude Opus 4.5 in Ask mode to help me break down the storage layout bit by bit. It was very helpful for breaking down the bit operations. It was also useful to write the boilerplate code for iterating through the storage slots. In general, it made a few mistakes and wanted to use other/older functions than the already available `storage_ref()` method in the provided `DatabaseRef`. In any case, I was very skeptical of its outputs and do not accept them until I understand them.

**Sources used:**
- https://docs.soliditylang.org/en/v0.8.26/internals/layout_in_storage.html
- https://docs.rs/revm/latest/revm/trait.DatabaseRef.html

### Writing to storage
For this challenge we are working purely on the in-memory database which we spin up with `spin_up_anvil_instance()`. Using the returned type we can derive a writable database `CacheDB`. Using in-memory CacheDB should be sufficient to prove the concept for this challenge.

To finish the challenge we need to flip the 224th bit of the in-memory value to 1. Other than that, the main piece of logic here is the `write_storage()` function. It is very straightforward following the docs in https://docs.rs/revm/19.2.0/revm/db/in_memory_db/struct.CacheDB.html#method.insert_account_storage.

**AI Usage:** Claude Opus 4.5 in Ask mode to help me wire the mask and flip bit 224. Also useful to write the boilerplate for the function `write_storage()`.

**Sources used:**
- https://docs.rs/revm/19.2.0/revm/db/in_memory_db/struct.CacheDB.html


### Verifying the solution
We need to call the `isSolved()` function on the smart contract at the end of `solve()` to verify our solution. I isolated the smart contract call in `call_is_solved()`

**Comment:** The function `call_is_solved()` is very rough as it is. It would be very impractical to add more smart contract calls in the same style. In a production app we would of course isolate all repeating code and put it behind an interface which can be called elegantly using one-liners. The current structure works for the challenge as we have only one contract call to make.

**AI Usage:** Claude Opus 4.5 in Ask mode to help me write the boilerplate `call_is_solved()`. This is a standard smart contract call so there is no need to dig too deep for the purposes of this challenge.

### Taking something like this to production
This is a very rough app and I was very minimal to deliver only what is needed. The challenge was big enough as it is so I did not go into making clean and testable structures and functions. The most important points I would consider and perfect when taking something like this to production are:
- Separation of different logic and code into modules following the DRY approach _(in general I like to use a lot of OOP approaches within a MVC structure to separate common logic and build maintainable code)_
- Full test coverage under a TDD approach
- Ensure that the build or unit tests break when contract logic changes and execution logic changes
- Full CI/CD where tests run on every push and PR
- Consider running this in a Kubernetes or Swarm clusters
- Key management considerations for deployment in production: this depends on the current setup you use