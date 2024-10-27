// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract InteropCenter {
    uint256 public interopMessagesSent;
    address public owner;

    // Constructor to set the owner
    constructor() {
        owner = msg.sender;
    }

    // Modifier to restrict access to only the owner
    modifier onlyOwner() {
        require(msg.sender == owner, "Not authorized");
        _;
    }

    // Type A - Interop Message

    // Interop Event
    event InteropMessageSent(
        bytes32 indexed msgHash,
        address indexed sender,
        bytes payload
    );

    struct InteropMessage {
        bytes data;
        address sender;
        uint256 sourceChainId;
        uint256 messageNum;
    }

    function sendInteropMessage(bytes memory data) public returns (bytes32) {
        // Increment message count
        interopMessagesSent++;

        // Create the InteropMessage struct
        InteropMessage memory message = InteropMessage({
            data: data,
            sender: msg.sender,
            sourceChainId: block.chainid,
            messageNum: interopMessagesSent
        });

        // Generate a hash for the message
        bytes32 msgHash = keccak256(
            abi.encodePacked(
                message.sender,
                message.sourceChainId,
                message.messageNum,
                message.data
            )
        );

        // Emit the event
        emit InteropMessageSent(msgHash, message.sender, message.data);

        // Return the message hash
        return msgHash;
    }

    // *** Trust-me-bro implementation of the interop ***
    // The real one should be using merkle proofs and root hashes from Gateway.

    // Mapping to store received message hashes
    mapping(bytes32 => bool) public receivedMessages;

    // Function to receive and store a message hash, restricted to the owner
    function receiveInteropMessage(bytes32 msgHash) public onlyOwner {
        receivedMessages[msgHash] = true;
    }

    // Function to verify if a message hash has been received
    function verifyInteropMessage(
        bytes32 msgHash,
        bytes calldata // proof
    ) public view returns (bool) {
        return receivedMessages[msgHash];
    }

    // Type B - Interop Call & Bundles

    // Mappings to store bundles by their ID
    mapping(uint256 => InteropBundle) public bundles;
    uint256 public nextBundleId = 1; // Unique identifier for each bundle

    struct InteropCall {
        address sourceSender;
        address destinationAddress;
        uint256 destinationChainId;
        bytes data;
        uint256 value;
    }

    struct InteropBundle {
        InteropCall[] calls;
        uint256 destinationChain;
    }

    // Function to start a new bundle
    function startBundle(uint256 destinationChain) public returns (uint256) {
        uint256 bundleId = nextBundleId++;
        bundles[bundleId] = InteropBundle({
            calls: new InteropCall[](0),
            destinationChain: destinationChain
        });

        return bundleId;
    }

    function addToBundle(
        uint256 bundleId,
        uint256 destinationChainId,
        address destinationAddress,
        bytes calldata payload,
        uint256 value
    ) public {
        // Ensure the bundle exists and has the correct destination chain
        require(
            bundles[bundleId].destinationChain == destinationChainId,
            "Destination chain mismatch"
        );

        // Create the InteropCall
        InteropCall memory newCall = InteropCall({
            sourceSender: msg.sender,
            destinationAddress: destinationAddress,
            destinationChainId: destinationChainId,
            data: payload,
            value: value
        });

        // Add the call to the bundle
        bundles[bundleId].calls.push(newCall);
    }

    // Function to finish and send the bundle
    function finishAndSendBundle(uint256 bundleId) public returns (bytes32) {
        // Retrieve the bundle and ensure it exists
        InteropBundle storage bundle = bundles[bundleId];
        require(bundle.calls.length > 0, "Bundle is empty");

        // Serialize the bundle data
        bytes memory serializedData = abi.encode(bundle);

        // Send the serialized data via interop message
        bytes32 msgHash = sendInteropMessage(serializedData);

        // Clean up the bundle storage
        delete bundles[bundleId];

        return msgHash;
    }

    function sendCall(
        uint256 destinationChain,
        address destinationAddress,
        bytes calldata payload,
        uint256 value
    ) public returns (bool) {}
}
