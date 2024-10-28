// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "../lib/forge-std/src/Script.sol";
import {Vm} from "../lib/forge-std/src/Vm.sol";
import {InteropCenter} from "../src/InteropCenter.sol";
import "../src/Greeter.sol";
import "../lib/forge-std/src/console2.sol";

contract InteropScript is Script {
    InteropCenter public interopCenter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        {
            // Send an interop message
            bytes memory payload = "Example payload";
            bytes32 msgHash = interopCenter.sendInteropMessage(payload);

            // Receive the message as the owner
            interopCenter.receiveInteropMessage(msgHash);

            // Verify the message
            // Create a sample proof (could be any bytes value)
            bytes memory proof = "Trust me bro";

            bool isVerified = interopCenter.verifyInteropMessage(
                msgHash,
                proof
            );

            console.log("Interop center", address(interopCenter));
            console.logBytes32(msgHash);

            // Output the verification result
            console.log("Message hash verified:", isVerified);
        }
        {
            // Step 1: Start a new bundle with a specified destination chain ID
            uint256 destinationChainId = 1001; // Example destination chain ID
            uint256 bundleId = interopCenter.startBundle(destinationChainId);
            console.log("Started bundle with ID:", bundleId);

            // Step 2: Add a call to the bundle
            address destinationAddress = address(0x1234); // Example destination address
            bytes memory payload = "Example payload";
            uint256 callValue = 1 ether;
            interopCenter.addToBundle(
                bundleId,
                destinationChainId,
                destinationAddress,
                payload,
                callValue
            );
            console.log("Added call to bundle ID:", bundleId);

            // Step 3: Finish and send the bundle
            bytes32 msgHash = interopCenter.finishAndSendBundle(bundleId);
            console.logBytes32(msgHash); // Log the message hash of the sent bundle
        }
        vm.stopBroadcast();
    }
}

contract InteropE2EBundle is Script {
    Greeter public greeter;
    InteropCenter public interopCenter;
    InteropCenter public destinationInteropCenter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        // Step 1: Deploy Greeter contract
        greeter = new Greeter();
        console2.log("Deployed Greeter at:", address(greeter));

        // Step 2: Deploy InteropCenter contract
        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        // TODO - deploy separate in the future.
        destinationInteropCenter = interopCenter;

        // Step 3: Add the InteropCenter as a trusted source for chain 260

        destinationInteropCenter.addTrustedSource(260, address(interopCenter));

        console2.log(
            "Added InteropCenter as trusted source for chain 260",
            address(interopCenter)
        );

        // Step 4: Prepare an InteropCall to set a greeting on the Greeter contract
        bytes memory payload = abi.encodeWithSignature(
            "setGreeting(string)",
            "Hello from chain 260!"
        );

        vm.recordLogs();

        // Use the sendCall function to create a bundle, add the call, and send it
        bytes32 sentMsgHash = interopCenter.sendCall(
            260,
            address(greeter),
            payload,
            0
        );

        console2.log("interopCenter Call sent");
        console2.logBytes32(sentMsgHash);
        // Pretend that destination got the message.
        destinationInteropCenter.receiveInteropMessage(sentMsgHash);

        // Step 5: Capture the InteropMessageSent event to retrieve msgHash and payload
        vm.stopBroadcast();

        // Fetch the emitted logs
        Vm.Log[] memory entries = vm.getRecordedLogs();
        bytes32 msgHash;
        bytes memory eventPayload;

        for (uint i = 0; i < entries.length; i++) {
            if (
                entries[i].topics[0] ==
                keccak256("InteropMessageSent(bytes32,address,bytes)")
            ) {
                msgHash = bytes32(entries[i].topics[1]);
                //eventPayload = entries[i].data; // Directly captures the payload as the event data
                eventPayload = abi.decode(entries[i].data, (bytes));

                break;
            }
        }

        require(msgHash != bytes32(0), "InteropMessageSent event not found");
        console.log("found msg");
        console.logBytes32(msgHash);

        vm.startBroadcast();

        // Decode the serialized InteropMessage from the event payload

        // Step 6: Execute the InteropBundle on the InteropCenter
        InteropCenter.InteropMessage memory interopMessage = abi.decode(
            eventPayload,
            (InteropCenter.InteropMessage)
        );

        interopCenter.executeInteropBundle(interopMessage, "0x"); // Pass an empty proof for simplicity

        console.log("Executed InteropBundle:");
        console.logBytes32(msgHash);

        console.log(greeter.getGreeting());

        vm.stopBroadcast();
    }
}

contract InteropE2ETx is Script {
    Greeter public greeter;
    InteropCenter public interopCenter;
    InteropCenter public destinationInteropCenter;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        // Step 1: Deploy Greeter contract
        greeter = new Greeter();
        console2.log("Deployed Greeter at:", address(greeter));

        // Step 2: Deploy InteropCenter contract
        interopCenter = new InteropCenter();
        console2.log("Deployed InteropCenter at:", address(interopCenter));

        // TODO - deploy separate in the future.
        destinationInteropCenter = interopCenter;

        // Step 3: Add the InteropCenter as a trusted source for chain 260

        destinationInteropCenter.addTrustedSource(260, address(interopCenter));

        console2.log(
            "Added InteropCenter as trusted source for chain 260",
            address(interopCenter)
        );

        // Step 4: Prepare an InteropCall to set a greeting on the Greeter contract
        bytes memory payload = abi.encodeWithSignature(
            "setGreeting(string)",
            "Hello from chain 260!"
        );

        vm.recordLogs();

        // Use the sendCall function to create a bundle, add the call, and send it
        bytes32 sentMsgHash = interopCenter.sendCall(
            260,
            address(greeter),
            payload,
            0
        );

        console2.log("interopCenter Call sent");
        console2.logBytes32(sentMsgHash);
        // Pretend that destination got the message.
        destinationInteropCenter.receiveInteropMessage(sentMsgHash);

        // Now create the transaction

        bytes32 txMsgHash = interopCenter.sendInteropTransaction(
            260,
            10000000, // gas limit
            10000000, // gas price
            0, // value
            sentMsgHash, // bundle hash
            bytes32(0), // feed bundle
            address(0), // destination paymaster
            ""
        );
        console2.log("interopCenter Tx sent");
        console2.logBytes32(txMsgHash);

        // Step 5: Capture the InteropMessageSent event to retrieve msgHash and payload
        vm.stopBroadcast();

        // Fetch the emitted logs
        Vm.Log[] memory entries = vm.getRecordedLogs();
        bytes32 msgHash;
        bytes memory eventPayload;

        for (uint i = 0; i < entries.length; i++) {
            if (
                entries[i].topics[0] ==
                keccak256("InteropMessageSent(bytes32,address,bytes)")
            ) {
                msgHash = bytes32(entries[i].topics[1]);
                eventPayload = abi.decode(entries[i].data, (bytes));
                break;
            }
        }

        require(msgHash != bytes32(0), "InteropMessageSent event not found");
        console.log("found msg");
        console.logBytes32(msgHash);

        vm.startBroadcast();

        // Decode the serialized InteropMessage from the event payload

        // Step 6: Execute the InteropBundle on the InteropCenter
        //InteropCenter.InteropMessage memory interopMessage = abi.decode(
        //    eventPayload,
        //    (InteropCenter.InteropMessage)
        //);

        //interopCenter.executeInteropBundle(interopMessage, "0x"); // Pass an empty proof for simplicity

        //console.log("Executed InteropBundle:");
        //console.logBytes32(msgHash);

        //console.log(greeter.getGreeting());

        //vm.stopBroadcast();
    }
}
