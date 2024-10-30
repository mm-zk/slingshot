// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../lib/forge-std/src/console2.sol";

import "../lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";
import "../lib/openzeppelin-contracts/contracts/access/Ownable.sol";

import {InteropCenter} from "../src/InteropCenter.sol";

contract PaymasterToken is ERC20, Ownable {
    address public interopAddress;

    constructor(
        address _interopAddress
    ) ERC20("CrossChainPaymasterToken", "CCPT") Ownable(msg.sender) {
        interopAddress = _interopAddress;
    }

    mapping(uint256 => address) public remoteAddresses;

    mapping(address => bool) public trustedAliasedAccounts;

    function addOtherBridge(
        uint256 sourceChainId,
        address sourceAddress
    ) public onlyOwner {
        address aliasedAddress = InteropCenter(interopAddress)
            .getAliasedAccount(sourceAddress, sourceChainId);
        trustedAliasedAccounts[aliasedAddress] = true;
        remoteAddresses[sourceChainId] = sourceAddress;

        console2.log("Setting address as trusted", aliasedAddress);
    }

    function receiveTokenFromRemote(
        address destinationAddress,
        uint256 amount
    ) public {
        console2.log("Minting tokens request from ", msg.sender);
        require(
            trustedAliasedAccounts[msg.sender],
            "msg sender is not trusted aliased account"
        );
        console2.log("Minting tokens for", destinationAddress);
        _mint(destinationAddress, amount);
    }

    function sendToRemote(
        uint256 bundleId,
        uint256 destinationChainId,
        address remoteRecipient,
        uint256 amount
    ) public {
        _burn(msg.sender, amount);
        bytes memory payload = abi.encodeWithSignature(
            "receiveTokenFromRemote(address,uint256)",
            remoteRecipient,
            amount
        );
        address destinationAddress = remoteAddresses[destinationChainId];
        require(
            destinationAddress != address(0),
            "No bridge on destination chain"
        );

        InteropCenter(interopAddress).addToBundle(
            bundleId,
            destinationChainId,
            destinationAddress,
            payload,
            0
        );
    }

    function buyTokens() public payable returns (uint256) {
        // exchange wei for tokens in  1-1
        uint256 tokens = msg.value;
        _mint(msg.sender, tokens);
        // For ease of use - allow interop to take any amount.
        _approve(msg.sender, interopAddress, type(uint256).max);

        return tokens;
    }

    function sudoApproveInterop(address someone) public {
        require(msg.sender == interopAddress, "Can only be called by interop");
        _approve(someone, interopAddress, type(uint256).max);
    }

    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }

    function burn(address from, uint256 amount) external onlyOwner {
        _burn(from, amount);
    }
}
