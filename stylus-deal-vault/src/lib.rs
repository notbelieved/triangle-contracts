#![cfg_attr(not(any(feature = "export-abi", test)), no_main)]

extern crate alloc;

use alloc::vec::Vec;
use stylus_sdk::call::transfer::transfer_eth;
use stylus_sdk::deploy::RawDeploy;
use stylus_sdk::prelude::{Call, *};

sol_interface! {
    interface IDealProxy {
        function withdrawTo(address to) external;
        function withdraw(address to, uint256 amount) external;
        function withdrawToken(address token, address to, uint256 amount) external;
    }
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
    interface IEscrow {
        function sweep(address token, address to, uint256 amount) external;
    }
}


const ESCROW_INIT_CODE: [u8; 82] = [
    // --- constructor (16 bytes) ---
    0x33, 0x60, 0x00, 0x55,             // CALLER PUSH1 0 SSTORE
    0x60, 0x42, 0x60, 0x10, 0x60, 0x00, // PUSH1 66 PUSH1 16 PUSH1 0
    0x39,                               // CODECOPY
    0x60, 0x42, 0x60, 0x00,             // PUSH1 66 PUSH1 0
    0xf3,                               // RETURN
    // --- runtime (66 bytes) ---
    // auth: require(msg.sender == sload(0))
    0x60, 0x00, 0x54, 0x33, 0x14,       // PUSH1 0 SLOAD CALLER EQ
    0x60, 0x0d, 0x57,                   // PUSH1 13 JUMPI
    0x60, 0x00, 0x60, 0x00, 0xfd,       // PUSH1 0 PUSH1 0 REVERT
    0x5b,                               // JUMPDEST (auth_ok)
    // build token.transfer(to, amount) in memory
    0x63, 0xa9, 0x05, 0x9c, 0xbb,       // PUSH4 0xa9059cbb
    0x60, 0xe0, 0x1b,                   // PUSH1 224 SHL
    0x60, 0x00, 0x52,                   // PUSH1 0 MSTORE
    0x60, 0x24, 0x35, 0x60, 0x04, 0x52, // cd[36] -> mem[4]  (to)
    0x60, 0x44, 0x35, 0x60, 0x24, 0x52, // cd[68] -> mem[36] (amount)
    // CALL(gas, token, 0, 0, 68, 0, 0)
    0x60, 0x00,                         // retLen
    0x60, 0x00,                         // retOff
    0x60, 0x44,                         // argsLen (68)
    0x60, 0x00,                         // argsOff
    0x60, 0x00,                         // value
    0x60, 0x04, 0x35,                   // cd[4] = token
    0x5a,                               // GAS
    0xf1,                               // CALL
    0x60, 0x3c, 0x57,                   // PUSH1 60 JUMPI
    0x60, 0x00, 0x60, 0x00, 0xfd,       // revert
    0x5b,                               // JUMPDEST (success)
    0x60, 0x00, 0x60, 0x00, 0xf3,       // RETURN
];

sol_storage! {
    #[entrypoint]
    pub struct DealVault {
        address owner;
        bool frozen;
        mapping(address => bool) frozen_proxy;
    }
}

#[public]
impl DealVault {
    pub fn init(&mut self, new_owner: stylus_sdk::alloy_primitives::Address) -> Result<(), Vec<u8>> {
        let zero = stylus_sdk::alloy_primitives::Address::ZERO;
        if self.owner.get() != zero {
            return Err(alloc::format!("AlreadyInitialized").into_bytes());
        }
        if new_owner == zero {
            return Err(alloc::format!("InvalidOwner").into_bytes());
        }
        self.owner.set(new_owner);
        Ok(())
    }

    #[payable]
    pub fn deposit(&mut self) -> Result<(), Vec<u8>> {
        Ok(())
    }

    pub fn withdraw(
        &mut self,
        to: stylus_sdk::alloy_primitives::Address,
        value: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen.get() {
            return Err(alloc::format!("Frozen").into_bytes());
        }
        if value == stylus_sdk::alloy_primitives::U256::ZERO {
            return Err(alloc::format!("ZeroAmount").into_bytes());
        }
        let balance = self.vm().balance(self.vm().contract_address());
        if balance < value {
            return Err(alloc::format!("InsufficientBalance").into_bytes());
        }
        transfer_eth(self.vm(), to, value)
            .map_err(|_| alloc::format!("TransferFailed").into_bytes())?;
        Ok(())
    }

    pub fn freeze(&mut self) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        self.frozen.set(true);
        Ok(())
    }

    pub fn unfreeze(&mut self) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        self.frozen.set(false);
        Ok(())
    }

    pub fn owner(&self) -> Result<stylus_sdk::alloy_primitives::Address, Vec<u8>> {
        Ok(self.owner.get())
    }

    pub fn frozen(&self) -> Result<bool, Vec<u8>> {
        Ok(self.frozen.get())
    }

    pub fn balance(&self) -> Result<stylus_sdk::alloy_primitives::U256, Vec<u8>> {
        Ok(self.vm().balance(self.vm().contract_address()))
    }

    #[receive]
    #[payable]
    pub fn receive(&mut self) -> Result<(), Vec<u8>> {
        Ok(())
    }

    pub fn release_from_proxy(
        &mut self,
        proxy: stylus_sdk::alloy_primitives::Address,
        seller: stylus_sdk::alloy_primitives::Address,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen_proxy.get(proxy) {
            return Err(alloc::format!("ProxyFrozen").into_bytes());
        }
        let iface = IDealProxy::new(proxy);
        let ctx = Call::new_mutating(self);
        iface.withdraw_to(self.vm(), ctx, seller).map_err(|_| alloc::format!("ProxyCallFailed").into_bytes())?;
        Ok(())
    }

    pub fn release_token_from_proxy(
        &mut self,
        proxy: stylus_sdk::alloy_primitives::Address,
        seller: stylus_sdk::alloy_primitives::Address,
        token: stylus_sdk::alloy_primitives::Address,
        amount: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen_proxy.get(proxy) {
            return Err(alloc::format!("ProxyFrozen").into_bytes());
        }
        if amount == stylus_sdk::alloy_primitives::U256::ZERO {
            return Ok(());
        }
        let iface = IDealProxy::new(proxy);
        let ctx = Call::new_mutating(self);
        iface
            .withdraw_token(self.vm(), ctx, token, seller, amount)
            .map_err(|_| alloc::format!("ProxyCallFailed").into_bytes())?;
        Ok(())
    }

    pub fn refund_from_proxy(
        &mut self,
        proxy: stylus_sdk::alloy_primitives::Address,
        to: stylus_sdk::alloy_primitives::Address,
        amount: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen_proxy.get(proxy) {
            return Err(alloc::format!("ProxyFrozen").into_bytes());
        }
        if amount == stylus_sdk::alloy_primitives::U256::ZERO {
            return Ok(());
        }
        let iface = IDealProxy::new(proxy);
        let ctx = Call::new_mutating(self);
        iface.withdraw(self.vm(), ctx, to, amount).map_err(|_| alloc::format!("ProxyCallFailed").into_bytes())?;
        Ok(())
    }

    pub fn release_token(
        &mut self,
        token: stylus_sdk::alloy_primitives::Address,
        to: stylus_sdk::alloy_primitives::Address,
        amount: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen.get() {
            return Err(alloc::format!("Frozen").into_bytes());
        }
        if amount == stylus_sdk::alloy_primitives::U256::ZERO {
            return Ok(());
        }
        let erc20 = IERC20::new(token);
        let ctx = Call::new_mutating(self);
        erc20
            .transfer(self.vm(), ctx, to, amount)
            .map_err(|_| alloc::format!("TokenTransferFailed").into_bytes())?;
        Ok(())
    }

    pub fn create_escrow(
        &mut self,
        salt: [u8; 32],
    ) -> Result<stylus_sdk::alloy_primitives::Address, Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        let salt_b256 = stylus_sdk::alloy_primitives::B256::from(salt);
        let deployer = RawDeploy::new().salt(salt_b256);
        let addr = unsafe {
            deployer.deploy(self.vm(), &ESCROW_INIT_CODE, stylus_sdk::alloy_primitives::U256::ZERO)
        }
        .map_err(|_| alloc::format!("EscrowDeployFailed").into_bytes())?;
        Ok(addr)
    }

    pub fn predict_escrow(
        &self,
        salt: [u8; 32],
    ) -> Result<stylus_sdk::alloy_primitives::Address, Vec<u8>> {
        let factory = self.vm().contract_address();
        let code_hash = stylus_sdk::alloy_primitives::keccak256(&ESCROW_INIT_CODE);
        let mut buf = Vec::with_capacity(1 + 20 + 32 + 32);
        buf.push(0xff);
        buf.extend_from_slice(factory.as_slice());
        buf.extend_from_slice(&salt);
        buf.extend_from_slice(code_hash.as_slice());
        let hash = stylus_sdk::alloy_primitives::keccak256(&buf);
        Ok(stylus_sdk::alloy_primitives::Address::from_slice(&hash[12..32]))
    }

    pub fn release_from_escrow(
        &mut self,
        escrow: stylus_sdk::alloy_primitives::Address,
        token: stylus_sdk::alloy_primitives::Address,
        to: stylus_sdk::alloy_primitives::Address,
        amount: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        if self.frozen.get() {
            return Err(alloc::format!("Frozen").into_bytes());
        }
        if amount == stylus_sdk::alloy_primitives::U256::ZERO {
            return Ok(());
        }
        let iface = IEscrow::new(escrow);
        let ctx = Call::new_mutating(self);
        iface
            .sweep(self.vm(), ctx, token, to, amount)
            .map_err(|_| alloc::format!("SweepFailed").into_bytes())?;
        Ok(())
    }

    pub fn freeze_proxy(&mut self, proxy: stylus_sdk::alloy_primitives::Address) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        self.frozen_proxy.insert(proxy, true);
        Ok(())
    }

    pub fn unfreeze_proxy(&mut self, proxy: stylus_sdk::alloy_primitives::Address) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(alloc::format!("OnlyOwner").into_bytes());
        }
        self.frozen_proxy.insert(proxy, false);
        Ok(())
    }
}
