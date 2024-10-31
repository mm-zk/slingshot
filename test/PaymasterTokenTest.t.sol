// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {InteropCenter} from "../src/InteropCenter.sol";
import {PaymasterToken} from "../src/PaymasterToken.sol";
import {CrossPaymaster} from "../src/CrossPaymaster.sol";
import {Greeter} from "../src/Greeter.sol";

import "../src/Greeter.sol";
import "../lib/forge-std/src/console2.sol";
import {Transaction, TransactionHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/TransactionHelper.sol";

import {Test, console} from "../lib/forge-std/src/Test.sol";
import {TestExt} from "../lib/forge-zksync-std/src/TestExt.sol";
import {Vm} from "../lib/forge-std/src/Vm.sol";

contract TransactionConversion is Test, TestExt {
    InteropCenter public interopCenter;

    function setUp() public {}

    function test_Conversion() public {
        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        address interopOnSource = address(
            0x6Fb7817d183F7C84A546770338bf1F5d2111e43a
        );
        interopCenter.addTrustedSource(99, interopOnSource);

        address userSender = address(
            0x5f3649BBfCE8f62738c8346588e0F62469087d9e
        );
        address localAliased = interopCenter.getAliasedAccount(userSender, 99);

        InteropCenter.InteropMessage memory executionBundle = InteropCenter
            .InteropMessage({
                data: hex"0102",
                sender: address(0),
                sourceChainId: 55,
                messageNum: 44
            });

        InteropCenter.InteropMessage memory feeBundle = InteropCenter
            .InteropMessage({
                data: hex"010204",
                sender: address(0),
                sourceChainId: 55,
                messageNum: 45
            });

        InteropCenter.TransactionReservedStuff memory stuff = InteropCenter
            .TransactionReservedStuff({
                sourceChainSender: userSender,
                interopMessageSender: interopOnSource,
                sourceChainId: 99,
                messageNum: 0,
                destinationChainId: block.chainid,
                bundleHash: keccak256(abi.encode(executionBundle)),
                feesBundleHash: keccak256(abi.encode(feeBundle))
            });

        bytes memory proof = hex"";

        Transaction memory transaction = Transaction({
            txType: 113,
            from: uint256(uint160(localAliased)),
            to: uint256(uint160(address(interopCenter))),
            gasLimit: 0,
            gasPerPubdataByteLimit: 10000,
            maxFeePerGas: 0,
            maxPriorityFeePerGas: 0,
            paymaster: 0,
            nonce: 0,
            value: 0,
            reserved: [uint256(0), uint256(0), uint256(0), uint256(0)],
            data: abi.encodeWithSignature(
                "executeInteropBundle((bytes,address,uint256,uint256),bytes)",
                executionBundle,
                proof
            ),
            signature: abi.encode(stuff),
            factoryDeps: new bytes32[](0),
            paymasterInput: abi.encode(feeBundle),
            reservedDynamic: hex""
        });

        InteropCenter.InteropMessage memory message = interopCenter
            .transactionToInteropMessage(transaction);
        // TODO: compare hash.

        // And also verify.
        interopCenter.verifyPotentialTransaction(transaction);
    }
}

contract PaymasterScript is Test, TestExt {
    InteropCenter public interopCenter;
    PaymasterToken public paymasterToken;
    CrossPaymaster public crossPaymaster;

    Greeter public greeter;

    function setUp() public {}

    function test_Simple() public {
        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        paymasterToken = new PaymasterToken(address(interopCenter));
        console2.log("Deployed Paymaster token at:", address(paymasterToken));

        crossPaymaster = new CrossPaymaster(
            address(paymasterToken),
            address(interopCenter)
        );
        console2.log("Deployed Paymaster  at:", address(crossPaymaster));

        greeter = new Greeter();
        console2.log("Deployed greeter at:", address(greeter));

        greeter.setGreeting("Hello - first direct");

        console2.log(greeter.getGreeting());

        //bytes memory paymaster_encoded_input = abi.encodeWithSelector(
        //    bytes4(keccak256("general(bytes)")),
        //    bytes("0x")
        //);
        //vmExt.zkUsePaymaster(address(crossPaymaster), paymaster_encoded_input);

        //greeter.setGreeting("Hello - first paymaster");

        console2.log(greeter.getGreeting());
    }
}

contract TokensInterop is Test, TestExt {
    InteropCenter public interopCenter;
    PaymasterToken public paymasterToken;
    CrossPaymaster public crossPaymaster;

    InteropCenter public interopCenter2;
    PaymasterToken public paymasterToken2;
    CrossPaymaster public crossPaymaster2;

    Greeter public greeter;

    function setUp() public {}

    function test_Tokens() public {
        // Chain 1
        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));
        paymasterToken = new PaymasterToken(address(interopCenter));
        console2.log("Deployed Paymaster token at:", address(paymasterToken));
        crossPaymaster = new CrossPaymaster(
            address(paymasterToken),
            address(interopCenter)
        );
        console2.log("Deployed Paymaster  at:", address(crossPaymaster));

        // Chain 2
        interopCenter2 = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter2));
        paymasterToken2 = new PaymasterToken(address(interopCenter2));
        console2.log("Deployed Paymaster token at:", address(paymasterToken2));
        crossPaymaster2 = new CrossPaymaster(
            address(paymasterToken2),
            address(interopCenter2)
        );
        console2.log("Deployed Paymaster  at:", address(crossPaymaster2));

        uint256 chainId = block.chainid;
        // Unfortunately forge script doesn't allow to change the chain ids..
        uint256 chainId2 = block.chainid;

        interopCenter.addTrustedSource(chainId, address(interopCenter2));
        interopCenter2.addTrustedSource(chainId, address(interopCenter));

        paymasterToken.addOtherBridge(chainId2, address(paymasterToken2));
        paymasterToken2.addOtherBridge(chainId, address(paymasterToken));

        // Mint some tokens on '1'.
        address localAddr = 0xfA45863d774e9FD27DEC19c418d9d7eB4C8a27E9;

        paymasterToken.mint(localAddr, 100);

        console2.log("Tokens on 1:", paymasterToken.balanceOf(localAddr));

        uint256 bundleId = interopCenter.startBundle(chainId2);

        address remoteRecipient = interopCenter2.getAliasedAccount(
            localAddr,
            chainId
        );

        vm.prank(localAddr);
        paymasterToken.sendToRemote(bundleId, chainId2, remoteRecipient, 5);

        console2.log(
            "Tokens on 1 after bundle:",
            paymasterToken.balanceOf(localAddr)
        );

        vm.recordLogs();
        bytes32 bundleHash = interopCenter.finishAndSendBundle(bundleId);
        interopCenter2.receiveInteropMessage(bundleHash);

        Vm.Log[] memory logs = vm.getRecordedLogs();

        bytes32 msgHash;
        bytes memory eventPayload;

        for (uint256 i = 0; i < logs.length; i++) {
            Vm.Log memory log = logs[i];
            if (
                log.topics[0] ==
                keccak256("InteropMessageSent(bytes32,address,bytes)")
            ) {
                msgHash = bytes32(log.topics[1]);
                eventPayload = abi.decode(log.data, (bytes));
                console2.log("Got interop message");
                break;
            }
        }
        require(msgHash == bundleHash, "got wrong interop message");

        InteropCenter.InteropMessage memory interopMessage = abi.decode(
            eventPayload,
            (InteropCenter.InteropMessage)
        );

        // Now let's run it on chain 2.
        interopCenter2.executeInteropBundle(interopMessage, "0x");

        console2.log("Execution finished");

        console2.log(
            "Balance on destination",
            paymasterToken2.balanceOf(remoteRecipient)
        );
        require(paymasterToken2.balanceOf(remoteRecipient) == 5);

        greeter = new Greeter();
        console2.log("Deployed greeter at:", address(greeter));
    }
}
