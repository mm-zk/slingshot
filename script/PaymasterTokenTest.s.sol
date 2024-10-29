// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "../lib/forge-std/src/Script.sol";
import {Vm} from "../lib/forge-std/src/Vm.sol";
import {InteropCenter} from "../src/InteropCenter.sol";
import {PaymasterToken} from "../src/PaymasterToken.sol";
import {CrossPaymaster} from "../src/CrossPaymaster.sol";
import {Greeter} from "../src/Greeter.sol";

import "../src/Greeter.sol";
import "../lib/forge-std/src/console2.sol";
import {Transaction, TransactionHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/TransactionHelper.sol";

contract PaymasterScript is Script {
    InteropCenter public interopCenter;
    PaymasterToken public paymasterToken;
    CrossPaymaster public crossPaymaster;

    Greeter public greeter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        paymasterToken = new PaymasterToken();
        console2.log("Deployed Paymaster token at:", address(paymasterToken));

        crossPaymaster = new CrossPaymaster();
        console2.log("Deployed Paymaster  at:", address(crossPaymaster));

        greeter = new Greeter();
        console2.log("Deployed greeter at:", address(greeter));

        greeter.setGreeting("Hello - first direct");

        console2.log(greeter.getGreeting());
        vm.stopBroadcast();

        vm.prank(0xB1bB911f481388F8704BF3a3Bdc1D9b186e86037);

        bytes memory paymaster_encoded_input = abi.encodeWithSelector(
            bytes4(keccak256("general(bytes)")),
            bytes("0x")
        );
        vm.zkUsePaymaster(address(crossPaymaster), paymaster_encoded_input);

        vm.greeter.setGreeting("Hello - second");

        console2.log(greeter.getGreeting());
    }
}
