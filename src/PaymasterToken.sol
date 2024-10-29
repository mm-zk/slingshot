// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";
import "../lib/openzeppelin-contracts/contracts/access/Ownable.sol";

contract PaymasterToken is ERC20, Ownable {
    constructor()
        ERC20("CrossChainPaymasterToken", "CCPT")
        Ownable(msg.sender)
    {}

    function mint(address to, uint256 amount) external onlyOwner {
        _mint(to, amount);
    }

    function burn(address from, uint256 amount) external onlyOwner {
        _burn(from, amount);
    }
}
