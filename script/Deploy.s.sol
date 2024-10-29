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

contract Deploy is Script {
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

        crossPaymaster = new CrossPaymaster(address(paymasterToken));
        console2.log("Deployed Paymaster  at:", address(crossPaymaster));

        greeter = new Greeter();
        console2.log("Deployed greeter at:", address(greeter));

        // register preferred local paymaster.
        interopCenter.setPreferredPaymaster(
            block.chainid,
            address(crossPaymaster)
        );

        address payable paymasterPayable = payable(address(crossPaymaster));

        // This doesn't pass any value in broadcast mode.. ehh ...
        (bool success, ) = paymasterPayable.call{value: 50000}("");
        require(success, "Call failed");

        console2.log("Balance is ", paymasterPayable.balance);

        vm.stopBroadcast();
    }
}
