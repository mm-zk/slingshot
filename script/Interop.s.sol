// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "../lib/forge-std/src/Script.sol";
import {InteropCenter} from "../src/InteropCenter.sol";

contract InteropScript is Script {
    InteropCenter public interopCenter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        interopCenter = new InteropCenter();
        // Send an interop message
        bytes memory payload = "Example payload";
        bytes32 msgHash = interopCenter.sendInteropMessage(payload);

        // Receive the message as the owner
        interopCenter.receiveInteropMessage(msgHash);

        // Verify the message
        // Create a sample proof (could be any bytes value)
        bytes memory proof = "Trust me bro";

        bool isVerified = interopCenter.verifyInteropMessage(msgHash, proof);

        vm.stopBroadcast();
        console.log("Interop center", address(interopCenter));
        console.logBytes32(msgHash);

        // Output the verification result
        console.log("Message hash verified:", isVerified);
    }
}
