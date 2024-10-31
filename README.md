# Interop interface example.

## Development with forge script

forge script script/Interop.s.sol:InteropE2ETx --rpc-url http://localhost:8011 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync  --skip-simulation  --enable-eravm-extensions --broadcast



## Plan


[x] - Type A (InteropMessage)

[x] - Type B messaging

[x] - Type C messaging


[] - authorization verification in AA

[] - Example 'bridge' with assets movement

[x] - split script into 2 parts and run with 2 era test nodes.

[x] - add 'watch' option to the cli script

[] - add nullifiers

[] - Add some paymaster

### Current hacks

[] - cli is transferring '1 ether' to aliased accounts, as we don't have bridges yet

[] - cli is creating the aliased account when needed.

[] - no verification in the aliased accounts


## Execution info

forge script script/Interop.s.sol:InteropE2E --rpc-url http://localhost:8011 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync  --skip-simulation --broadcast



## Setup

**IMPORTANT**

Please use era-test-node from https://github.com/matter-labs/era-test-node/tree/mmzk_1019_for_slingshot branch.

for alloy-zksync: the latest from https://github.com/popzxc/alloy-zksync 



Start 2 era test nodes and deploy the interop center to both.

```shell

cargo run --release -- --port 8012 --chain-id 500
cargo run --release -- --port 8013 --chain-id 501

# deploy all the stuff - please check the InteropCenter deploy address afterwards (should be same on both)
forge script script/Deploy.s.sol:Deploy --rpc-url http://localhost:8012 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync  --skip-simulation  --enable-eravm-extensions --broadcast && forge script script/Deploy.s.sol:Deploy --rpc-url http://localhost:8013 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync  --skip-simulation  --enable-eravm-extensions --broadcast
```

(on both chains)
>   Deployed InteropCenter at: 0xTHIS_IS_INTEROP_ADDRESS

Run the CLI with the same private key, it will handle the message passing and transaction creation.


```shell
cargo run -- -r http://localhost:8012 0xTHIS_IS_INTEROP_ADDRESS -r http://localhost:8013 0xTHIS_IS_INTEROP_ADDRESS  --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --base-token-price 200000 --base-token-price 100 --paymaster-balance-cents 20000
```

### Examples how to trigger:

Creating 'type A' message:
```
cast send -r http://localhost:8012 0xTHIS_IS_INTEROP_ADDRESS "sendInteropMessage(bytes)" 0x12 --private-key 0x509ca2e9e6acf0ba086477910950125e698d4ea70fa6f63e000c5a22bda9361c
```


Creating 'type C' interop transaction (that would send over 100 value to empty address):

```
cast send -r http://localhost:8012 0xTHIS_IS_INTEROP_ADDRESS "requestInteropMinimal(uint256, address, bytes, uint256, uint256, uint256)" 501 0x8B912Dfa4Db5f44FB5B6c8A2BA8925f01DA322EE 0x 100 10000000 1000000000  --private-key 0x7becc4a46e0c3b512d380ca73a4c868f790d1055a7698f38fb3ca2b2ac97efbb 
```

Paying with tokens:

```
# this should return empty.
cast call -r http://localhost:8013 GREETER_ADDRESS "greeting()(string)"

cast calldata "setGreeting(string)" hello

> 0xa41368620000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000568656c6c6f000000000000000000000000000000000000000000000000000000

cast send -r http://localhost:8012 0xTHIS_IS_INTEROP  "requestInteropMinimalPayLocally(uint256, address, bytes, uint256, uint256)" 501 0xaAF5f437fB0524492886fbA64D703df15BF619AE  0xa41368620000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000568656c6c79000000000000000000000000000000000000000000000000000000 50000000 1000000000  --private-key 0x7becc4a46e0c3b512d380ca73a4c868f790d1055a7698f38fb3ca2b2ac97efbb --gas-limit 10000000000 --value YYYY

# this should return hello
cast call -r http://localhost:8013 GREETER_ADDRESS "greeting()(string)"
```




### Log

* Added basic paymaster - that accepts everything for now.
* Checked that alloy-zksync is supporting it (needed some small changes).
* Changed the deploy script to deploy the paymaster and paymaster tokens.

* rust code is fetching paymaster address from interop now.
* paymaster sees the paymaster tokens (but don't use them yet.)
* rust now refills the paymaster (not Aliased accounts anymore)

* paymaster addresses are now shared by interops.
* added tokens to paymasters - so it now actually calls the fee bundle, and collects the fees.

* typeC validation works on AA
* added typeC validation within paymaster.
* add ratios
* add nullifiers
* make rust code more 'stable' 


Next steps:
