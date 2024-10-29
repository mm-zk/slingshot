// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../lib/forge-std/src/console2.sol";

import "../lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";
import "../lib/openzeppelin-contracts/contracts/access/Ownable.sol";
import {IPaymaster, ExecutionResult, PAYMASTER_VALIDATION_SUCCESS_MAGIC} from "../lib/era-contracts/system-contracts/contracts/interfaces/IPaymaster.sol";
import {Transaction, TransactionHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/TransactionHelper.sol";

contract CrossPaymaster is IPaymaster {
    using TransactionHelper for *;

    constructor() payable {}

    function validateAndPayForPaymasterTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable returns (bytes4 magic, bytes memory context) {
        console2.log("using paymaster!!");
        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
        magic = PAYMASTER_VALIDATION_SUCCESS_MAGIC;
    }

    function postTransaction(
        bytes calldata _context,
        Transaction calldata _transaction,
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        ExecutionResult _txResult,
        uint256 _maxRefundedGas
    ) external payable {}

    receive() external payable {
        console2.log("inside receive");
    }

    fallback() external payable {
        console2.log("inside fallback");
    }
}
