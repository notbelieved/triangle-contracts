// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Escrow {
    address public vault;
    bool private _init;

    function init(address _vault) external {
        require(!_init, "AlreadyInit");
        require(_vault != address(0), "Zero");
        vault = _vault;
        _init = true;
    }

    function sweep(address token, address to, uint256 amount) external {
        require(msg.sender == vault, "OnlyVault");
        (bool ok, ) = token.call(abi.encodeWithSelector(0xa9059cbb, to, amount));
        require(ok, "TransferFailed");
    }
}

contract EscrowFactory {
    address public immutable escrowImpl;
    address public immutable vault;

    constructor(address _vault) {
        require(_vault != address(0), "Zero");
        vault = _vault;
        escrowImpl = address(new Escrow());
    }

    function createEscrow(bytes32 salt) external returns (address instance) {
        instance = _cloneDeterministic(escrowImpl, salt);
        Escrow(instance).init(vault);
    }

    function predictEscrow(bytes32 salt) external view returns (address) {
        return _predictAddress(escrowImpl, salt);
    }

    function _cloneDeterministic(address impl, bytes32 salt) internal returns (address instance) {
        assembly ("memory-safe") {
            mstore(0x00, or(shr(232, shl(96, impl)), 0x3d602d80600a3d3981f3363d3d373d3d3d363d73000000))
            mstore(0x20, or(shl(120, impl), 0x5af43d82803e903d91602b57fd5bf3))
            instance := create2(0, 0x09, 0x37, salt)
        }
        require(instance != address(0), "DeployFailed");
    }

    function _predictAddress(address impl, bytes32 salt) internal view returns (address predicted) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(add(ptr, 0x38), address())
            mstore(add(ptr, 0x24), 0x5af43d82803e903d91602b57fd5bf3ff)
            mstore(add(ptr, 0x14), impl)
            mstore(ptr, 0x3d602d80600a3d3981f3363d3d373d3d3d363d73)
            mstore(add(ptr, 0x58), salt)
            mstore(add(ptr, 0x78), keccak256(add(ptr, 0x0c), 0x37))
            predicted := and(keccak256(add(ptr, 0x43), 0x55), 0xffffffffffffffffffffffffffffffffffffffff)
        }
    }
}
