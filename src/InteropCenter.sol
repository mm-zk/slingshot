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

    function sendInteropMessage(bytes calldata data) public returns (bytes32) {
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
}
