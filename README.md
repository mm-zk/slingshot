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

Start 2 era test nodes and deploy the interop center to both.

```shell

cargo run --release -- --port 8012 --chain-id 500
cargo run --release -- --port 8013 --chain-id 501

forge create src/InteropCenter.sol:InteropCenter --private-key 0xb0680d66303a0163a19294f1ef8c95cd69a9d7902a4aca99c05f3e134e68a11a  --rpc-url http://localhost:8012 --zksync --enable-eravm-extensions
forge create src/InteropCenter.sol:InteropCenter --private-key 0xb0680d66303a0163a19294f1ef8c95cd69a9d7902a4aca99c05f3e134e68a11a  --rpc-url http://localhost:8013 --zksync --enable-eravm-extensions

```

(on both chains)
> Deployed to: 0x915148c3a7d97ecF4F741FfaaB8263F9D66F2d0c

Run the CLI with the same private key, it will handle the message passing and transaction creation.

```shell 
cargo run -- -r http://localhost:8012 0x915148c3a7d97ecF4F741FfaaB8263F9D66F2d0c -r http://localhost:8013 0x915148c3a7d97ecF4F741FfaaB8263F9D66F2d0c --private-key 0xb0680d66303a0163a19294f1ef8c95cd69a9d7902a4aca99c05f3e134e68a11a
```

### Examples how to trigger:

Creating 'type A' message:
```
cast send -r http://localhost:8012 0x915148c3a7d97ecF4F741FfaaB8263F9D66F2d0c "sendInteropMessage(bytes)" 0x12 --private-key 0x509ca2e9e6acf0ba086477910950125e698d4ea70fa6f63e000c5a22bda9361c
```


Creating 'type C' interop transaction (that would send over 100 value to empty address):

```
cast send -r http://localhost:8012 0xb03b0432524bF54dDefBC38033eEe3D8b6b154C4 "requestInteropMinimal(uint256, address, bytes, uint256, uint256, uint256)" 501 0x8B912Dfa4Db5f44FB5B6c8A2BA8925f01DA322EE 0x 100 10000000 1000000000  --private-key 0x7becc4a46e0c3b512d380ca73a4c868f790d1055a7698f38fb3ca2b2ac97efbb
```