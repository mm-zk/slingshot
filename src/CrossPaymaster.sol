// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../lib/forge-std/src/console2.sol";

import "../lib/openzeppelin-contracts/contracts/token/ERC20/ERC20.sol";
import "../lib/openzeppelin-contracts/contracts/access/Ownable.sol";
import {IPaymaster, ExecutionResult, PAYMASTER_VALIDATION_SUCCESS_MAGIC} from "../lib/era-contracts/system-contracts/contracts/interfaces/IPaymaster.sol";
import {Transaction, TransactionHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/TransactionHelper.sol";
import {PaymasterToken} from "../src/PaymasterToken.sol";
import {InteropCenter} from "../src/InteropCenter.sol";

contract CrossPaymaster is IPaymaster {
    using TransactionHelper for *;

    address public paymasterTokenAddress;
    address public interopCenterAddress;

    constructor(
        address _paymasterTokenAddress,
        address _interopCenterAddress
    ) payable {
        paymasterTokenAddress = _paymasterTokenAddress;
        interopCenterAddress = _interopCenterAddress;
    }

    function name() public view virtual returns (string memory) {
        return "SlingshotPaymaster";
    }

    function validateAndPayForPaymasterTransaction(
        bytes32, // _txHash,
        bytes32, // _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable returns (bytes4 magic, bytes memory) {
        // AAA - do not use msg.sender -- it is 'bootloader'..
        console2.log("using paymaster!!");

        // It should:
        // - check that full transaction is legit
        // - unpack the bundle from the paymaster input ? or signature?
        // - check the bundle
        // - execute the bundle
        // -- that bundle should have given user tokens - then grab these tokens
        // -- and then pay for the user.

        // Anyone could try to call this paymaster, to let's see that the transaction is really legit.
        InteropCenter(interopCenterAddress).verifyPotentialTransaction(
            _transaction
        );
        InteropCenter.InteropMessage memory message = InteropCenter(
            interopCenterAddress
        ).transactionToInteropMessage(_transaction);

        bytes32 msgHash = keccak256(abi.encode(message));
        console2.log("Computed msg hash");
        console2.logBytes32(msgHash);

        bytes memory transactionInteropProof = new bytes(0);

        require(
            InteropCenter(interopCenterAddress).verifyInteropMessage(
                msgHash,
                transactionInteropProof
            ),
            "interop message missing"
        );

        console2.log("message is legit - unpacking fee");
        InteropCenter.InteropMessage memory feeMessage = abi.decode(
            _transaction.paymasterInput,
            (InteropCenter.InteropMessage)
        );
        console2.log("Fee unpacked");
        // Todo - proof should be taken from within signature.
        bytes memory proof = new bytes(0);

        // executing fee bundle.
        InteropCenter(interopCenterAddress).executeInteropBundle(
            feeMessage,
            proof
        );
        console2.log("Fee executed");

        address from = address(uint160(_transaction.from));

        console2.log("Taking assets from", from);

        uint256 currentBalance = PaymasterToken(paymasterTokenAddress)
            .balanceOf(from);
        console2.log("Current balance", currentBalance);

        uint256 tokensToPay = _transaction.maxFeePerGas * _transaction.gasLimit;
        console2.log("Charging user ", tokensToPay);
        PaymasterToken(paymasterTokenAddress).transferFrom(
            from,
            address(this),
            tokensToPay
        );
        console2.log("Paying bootloader");

        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
        console2.log("Paymaster is done");
        return (PAYMASTER_VALIDATION_SUCCESS_MAGIC, new bytes(0));
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
