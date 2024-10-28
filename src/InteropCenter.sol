// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../lib/forge-std/src/console2.sol";

import {IAccount, ACCOUNT_VALIDATION_SUCCESS_MAGIC} from "../lib/era-contracts/system-contracts/contracts/interfaces/IAccount.sol";
import {Transaction, TransactionHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/TransactionHelper.sol";
import {Utils} from "../lib/era-contracts/system-contracts/contracts/libraries/Utils.sol";
import {BOOTLOADER_FORMAL_ADDRESS, DEPLOYER_SYSTEM_CONTRACT, NONCE_HOLDER_SYSTEM_CONTRACT, INonceHolder} from "../lib/era-contracts/system-contracts/contracts/Constants.sol";
import {EfficientCall} from "../lib/era-contracts/system-contracts/contracts/libraries/EfficientCall.sol";
import {SystemContractHelper} from "../lib/era-contracts/system-contracts/contracts/libraries/SystemContractHelper.sol";
import {IContractDeployer} from "../lib/era-contracts/system-contracts/contracts/ContractDeployer.sol";
import {SystemContractsCaller} from "../lib/era-contracts/system-contracts/contracts/libraries/SystemContractsCaller.sol";

contract InteropCenter {
    bytes1 constant BUNDLE_PREFIX = 0x01;
    bytes1 constant TRANSACTION_PREFIX = 0x02;

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

    function sendInteropMessage(bytes memory data) external returns (bytes32) {
        // Increment message count
        interopMessagesSent++;

        // Create the InteropMessage struct
        InteropMessage memory message = InteropMessage({
            data: data,
            sender: msg.sender,
            sourceChainId: block.chainid,
            messageNum: interopMessagesSent
        });

        console2.log("Sending interop from ", msg.sender);
        console2.log("Interop id: ", interopMessagesSent);

        // Serialize the entire InteropMessage struct
        bytes memory serializedMessage = abi.encode(message);

        // Calculate the msgHash directly from the serialized message
        bytes32 msgHash = keccak256(serializedMessage);

        // Emit the event with the serialized message as the payload
        emit InteropMessageSent(msgHash, message.sender, serializedMessage);

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
        bytes memory // proof
    ) public view returns (bool) {
        return receivedMessages[msgHash];
    }

    // Type B - Interop Call & Bundles

    // Struct for storage without dynamic array (as solidity doesn't support it)
    struct StoredInteropBundle {
        uint256 destinationChain;
    }
    // Mappings to store bundles by their ID
    mapping(uint256 => StoredInteropBundle) public bundles;
    mapping(uint256 => InteropCall[]) public bundleCalls;

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

        // Store only the destination chain in the storage mapping
        bundles[bundleId] = StoredInteropBundle({
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
        bundleCalls[bundleId].push(newCall);
    }

    // Function to finish and send the bundle
    function finishAndSendBundle(uint256 bundleId) public returns (bytes32) {
        // Ensure the bundle exists and has calls
        require(
            bundles[bundleId].destinationChain != 0,
            "Bundle does not exist"
        );
        require(bundleCalls[bundleId].length > 0, "Bundle is empty");

        // Prepare the full InteropBundle in memory for serialization
        InteropBundle memory fullBundle = InteropBundle({
            calls: bundleCalls[bundleId],
            destinationChain: bundles[bundleId].destinationChain
        });

        // Serialize the bundle data
        bytes memory serializedData = abi.encodePacked(
            InteropCenter.BUNDLE_PREFIX,
            abi.encode(fullBundle)
        );

        // Send the serialized data via interop message
        bytes32 msgHash = InteropCenter(address(this)).sendInteropMessage(
            serializedData
        );

        // Clean up
        delete bundles[bundleId];
        delete bundleCalls[bundleId];

        return msgHash;
    }

    function sendCall(
        uint256 destinationChain,
        address destinationAddress,
        bytes calldata payload,
        uint256 value
    ) public returns (bytes32) {
        // Step 1: Start a new bundle
        uint256 bundleId = startBundle(destinationChain);

        // Step 2: Add a call to the bundle
        addToBundle(
            bundleId,
            destinationChain,
            destinationAddress,
            payload,
            value
        );

        // Step 3: Finish and send the bundle
        return finishAndSendBundle(bundleId);
    }

    // Mapping to store trusted sources by chain ID.
    // In reality - we'll be trusting the 'fixed' pre-deployed addresses on each chain.
    mapping(uint256 => address) public trustedSources;
    // Add a trusted source for a given chain ID
    function addTrustedSource(
        uint256 sourceChainId,
        address trustedSender
    ) public onlyOwner {
        trustedSources[sourceChainId] = trustedSender;
    }

    // Gets aliased account that is controlled by source account on the current chain id.
    function getAliasedAccount(address sourceAccount) public returns (address) {
        bytes32 salt = keccak256(
            abi.encodePacked(block.chainid, sourceAccount)
        );
        return _getZKSyncCreate2Address(salt);
    }

    function executeInteropBundle(
        InteropMessage memory message,
        bytes memory proof
    ) public {
        // Verify the message sender is a trusted source

        require(
            trustedSources[message.sourceChainId] == message.sender,
            "Untrusted source"
        );
        console2.log("inside ");
        require(
            verifyInteropMessage(keccak256(abi.encode(message)), proof),
            "Message not verified"
        );

        // Deserialize the InteropBundle from message data
        bytes1 prefix = message.data[0];
        require(
            prefix == InteropCenter.BUNDLE_PREFIX,
            "Wrong prefix - expected bundle prefix"
        );

        bytes memory data = message.data;
        assembly {
            // Add 1 to skip the first byte and directly decode the rest
            data := add(data, 0x1)
        }
        InteropBundle memory bundle = abi.decode(data, (InteropBundle));
        require(bundle.destinationChain == block.chainid, "wrong chain id");

        for (uint256 i = 0; i < bundle.calls.length; i++) {
            InteropCall memory interopCall = bundle.calls[i];

            // Generate the unique address for the account using CREATE2
            bytes32 salt = keccak256(
                abi.encodePacked(
                    message.sourceChainId,
                    interopCall.sourceSender
                )
            );

            console2.log("creation hash");
            console2.logBytes32(keccak256(type(InteropAccount).creationCode));

            address payable accountAddress = payable(
                _getZKSyncCreate2Address(salt)
            );
            console2.log("Aliased account ", accountAddress);

            // If account does not exist, deploy it
            if (!isContract(accountAddress)) {
                console2.log("aliased account missing - deploy new one");

                address payable contractDeployer = payable(
                    0x0000000000000000000000000000000000008006
                );
                bytes32 bytecodeHash = getZKSyncBytecodeHash(
                    type(InteropAccount).creationCode
                );

                SystemContractsCaller.systemCallWithPropagatedRevert(
                    uint32(gasleft()),
                    contractDeployer,
                    0,
                    abi.encodeCall(
                        IContractDeployer.create2Account,
                        (
                            salt,
                            bytecodeHash,
                            "",
                            IContractDeployer.AccountAbstractionVersion.Version1
                        )
                    )
                );
            }

            // Call the interop function on the account
            InteropAccount(accountAddress).executeInteropCall(interopCall);
        }
    }

    // Helper to compute the CREATE2 address
    function _getCreate2Address(bytes32 salt) internal view returns (address) {
        return
            address(
                uint160(
                    uint256(
                        keccak256(
                            abi.encodePacked(
                                bytes1(0xff),
                                address(this),
                                salt,
                                keccak256(type(InteropAccount).creationCode)
                            )
                        )
                    )
                )
            );
    }

    function getZKSyncBytecodeHash(
        bytes memory code
    ) internal pure returns (bytes32) {
        require(code.length >= 100, "Data must be at least 100 bytes");

        bytes32 result;
        // Load 32 bytes starting from the 68th byte
        assembly {
            result := mload(add(code, 68)) // 68 + 32 = 100
        }
        return result;
    }

    function _getZKSyncCreate2Address(
        bytes32 salt
    ) internal view returns (address) {
        return
            address(
                uint160(
                    uint256(
                        keccak256(
                            bytes.concat(
                                keccak256("zksyncCreate2"), // zkSync-specific prefix
                                bytes32(uint256(uint160(address(this)))), // Address of the contract deployer
                                salt, // Salt for the deployment
                                getZKSyncBytecodeHash(
                                    type(InteropAccount).creationCode
                                ), // Hash of the bytecode
                                keccak256("") // Hash of the constructor input data
                            )
                        )
                    )
                )
            );
    }

    // Check if an address is a contract
    function isContract(address addr) internal view returns (bool) {
        uint256 size;
        assembly {
            size := extcodesize(addr)
        }
        return size > 0;
    }

    // Transactions

    struct InteropTransaction {
        address sourceChainSender;
        uint256 destinationChain;
        uint256 gasLimit;
        uint256 gasPrice;
        uint256 value;
        bytes32 bundleHash;
        bytes32 feesBundleHash;
        address destinationPaymaster;
        bytes destinationPaymasterInput;
    }

    // Function to send interop transaction
    function sendInteropTransaction(
        uint256 destinationChain,
        uint256 gasLimit,
        uint256 gasPrice,
        uint256 value,
        bytes32 bundleHash,
        bytes32 feesBundleHash,
        address destinationPaymaster,
        bytes calldata destinationPaymasterInput
    ) external returns (bytes32) {
        // Create the InteropTransaction struct
        InteropTransaction memory transaction = InteropTransaction({
            sourceChainSender: msg.sender,
            destinationChain: destinationChain,
            gasLimit: gasLimit,
            gasPrice: gasPrice,
            value: value,
            bundleHash: bundleHash,
            feesBundleHash: feesBundleHash,
            destinationPaymaster: destinationPaymaster,
            destinationPaymasterInput: destinationPaymasterInput
        });

        // Serialize the struct
        bytes memory serializedTransaction = abi.encodePacked(
            InteropCenter.TRANSACTION_PREFIX,
            abi.encode(transaction)
        );

        bytes32 msgHash = InteropCenter(address(this)).sendInteropMessage(
            serializedTransaction
        );

        return msgHash;
    }
}

contract InteropAccount is IAccount {
    using TransactionHelper for *;

    address public trustedInteropCenter;

    // Constructor to set the trusted interop center
    constructor() {
        trustedInteropCenter = msg.sender;
    }

    // Execute function to forward interop call
    function executeInteropCall(
        InteropCenter.InteropCall calldata interopCall
    ) external {
        require(msg.sender == trustedInteropCenter, "Untrusted interop center");
        console2.log("Inside aliased account", address(this));
        console2.log("destination", interopCall.destinationAddress);

        // Forward the call to the destination address
        (bool success, ) = interopCall.destinationAddress.call{
            value: interopCall.value
        }(interopCall.data);
        require(success, "Interop call failed");
    }

    function validateTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable returns (bytes4 magic) {
        SystemContractsCaller.systemCallWithPropagatedRevert(
            uint32(gasleft()),
            address(NONCE_HOLDER_SYSTEM_CONTRACT),
            0,
            abi.encodeCall(
                INonceHolder.incrementMinNonceIfEquals,
                (_transaction.nonce)
            )
        );

        // TODO: add authorization
        magic = ACCOUNT_VALIDATION_SUCCESS_MAGIC;
    }

    function executeTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable {
        address to = address(uint160(_transaction.to));
        uint128 value = Utils.safeCastToU128(_transaction.value);
        bytes calldata data = _transaction.data;
        uint32 gas = Utils.safeCastToU32(gasleft());

        // Note, that the deployment method from the deployer contract can only be called with a "systemCall" flag.
        bool isSystemCall;
        if (to == address(DEPLOYER_SYSTEM_CONTRACT) && data.length >= 4) {
            bytes4 selector = bytes4(data[:4]);
            // Check that called function is the deployment method,
            // the others deployer method is not supposed to be called from the default account.
            isSystemCall =
                selector == DEPLOYER_SYSTEM_CONTRACT.create.selector ||
                selector == DEPLOYER_SYSTEM_CONTRACT.create2.selector ||
                selector == DEPLOYER_SYSTEM_CONTRACT.createAccount.selector ||
                selector == DEPLOYER_SYSTEM_CONTRACT.create2Account.selector;
        }
        bool success = EfficientCall.rawCall({
            _gas: gas,
            _address: to,
            _value: value,
            _data: data,
            _isSystem: isSystemCall
        });
        if (!success) {
            EfficientCall.propagateRevert();
        }
    }

    // There is no point in providing possible signed hash in the `executeTransactionFromOutside` method,
    // since it typically should not be trusted.
    function executeTransactionFromOutside(
        Transaction calldata _transaction
    ) external payable {
        revert();
    }

    function payForTransaction(
        bytes32 _txHash,
        bytes32 _suggestedSignedHash,
        Transaction calldata _transaction
    ) external payable {
        bool success = _transaction.payToTheBootloader();
        require(success, "Failed to pay the fee to the operator");
    }

    function prepareForPaymaster(
        bytes32 _txHash,
        bytes32 _possibleSignedHash,
        Transaction calldata _transaction
    ) external payable {
        revert();
    }

    modifier ignoreNonBootloader() {
        if (msg.sender != BOOTLOADER_FORMAL_ADDRESS) {
            // If function was called outside of the bootloader, behave like an EOA.
            assembly {
                return(0, 0)
            }
        }
        // Continue execution if called from the bootloader.
        _;
    }

    /**
     * @dev Simulate the behavior of the EOA if it is called via `delegatecall`.
     * Thus, the default account on a delegate call behaves the same as EOA on Ethereum.
     * If all functions will use this modifier AND the contract will implement an empty payable fallback()
     * then the contract will be indistinguishable from the EOA when called.
     */
    modifier ignoreInDelegateCall() {
        address codeAddress = SystemContractHelper.getCodeAddress();
        if (codeAddress != address(this)) {
            // If the function was delegate called, behave like an EOA.
            assembly {
                return(0, 0)
            }
        }

        // Continue execution if not delegate called.
        _;
    }

    fallback() external payable ignoreInDelegateCall {
        // fallback of default account shouldn't be called by bootloader under no circumstances
        assert(msg.sender != BOOTLOADER_FORMAL_ADDRESS);

        // If the contract is called directly, behave like an EOA
    }

    receive() external payable {
        // If the contract is called directly, behave like an EOA
    }
}
