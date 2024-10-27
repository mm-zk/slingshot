// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Greeter {
    string public greeting;
    address public lastSetter;

    // Event to log whenever a new greeting is set
    event GreetingSet(address indexed setter, string newGreeting);

    // Function to set the greeting message
    function setGreeting(string calldata newGreeting) public {
        greeting = newGreeting;
        lastSetter = msg.sender;
        emit GreetingSet(msg.sender, newGreeting);
    }

    // Function to get the current greeting message
    function getGreeting() public view returns (string memory) {
        return greeting;
    }

    // Function to get the address of the last person who set the greeting
    function getLastSetter() public view returns (address) {
        return lastSetter;
    }
}
