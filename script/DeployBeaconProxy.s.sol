// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../lib/openzeppelin-contracts/contracts/proxy/beacon/UpgradeableBeacon.sol";
import "../lib/openzeppelin-contracts/contracts/proxy/beacon/BeaconProxy.sol";
import "../src/MyImplementation.sol";
import {Script, console} from "forge-std/Script.sol";

contract DeployBeaconProxy is Script {
    function run() public returns (address beacon, address proxy) {
        // Deploy the initial implementation contract
        vm.startBroadcast();

        MyImplementation implementation = new MyImplementation();

        // Deploy the beacon with the implementation address
        UpgradeableBeacon upgradeableBeacon = new UpgradeableBeacon(
            address(implementation),
            tx.origin
        );

        // Deploy the proxy, pointing it to the beacon

        BeaconProxy beaconProxy = new BeaconProxy(
            address(upgradeableBeacon),
            abi.encodeWithSelector(MyImplementation.setValue.selector, 42) // Initializes value to 42
        );
        //MyImplementation beaconProxy = new MyImplementation();

        vm.stopBroadcast();

        // Return beacon and proxy addresses for verification
        return (address(upgradeableBeacon), address(beaconProxy));
    }
}
