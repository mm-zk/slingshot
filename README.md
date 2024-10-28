# TODO

To deploy example -- remember about skip-simulation

forge script script/DeployBeaconProxy.s.sol:DeployBeaconProxy --rpc-url http://localhost:8011 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync --broadcast  --skip-simulation


## Plan


[x] - Type A (InteropMessage)

[x] - Type B messaging

[x] - Type C messaging


[] - authorization verification in AA

[] - Example 'bridge' with assets movement

[] - split script into 2 parts and run with 2 era test nodes.


## Execution info

forge script script/Interop.s.sol:InteropE2E --rpc-url http://localhost:8011 --private-key 0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e --zksync  --skip-simulation --broadcast