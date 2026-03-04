#![cfg_attr(not(any(feature = "export-abi", test)), no_main)]

extern crate alloc;

use stylus_sdk::prelude::{Call, *};

sol_interface! {
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

sol_storage! {
    #[entrypoint]
    pub struct Escrow {
        address vault;
    }
}

#[public]
impl Escrow {
    pub fn init(
        &mut self,
        new_vault: stylus_sdk::alloy_primitives::Address,
    ) -> Result<(), Vec<u8>> {
        let zero = stylus_sdk::alloy_primitives::Address::ZERO;
        if self.vault.get() != zero {
            return Err(alloc::format!("AlreadyInitialized").into_bytes());
        }
        if new_vault == zero {
            return Err(alloc::format!("ZeroAddress").into_bytes());
        }
        self.vault.set(new_vault);
        Ok(())
    }

    pub fn vault(&self) -> Result<stylus_sdk::alloy_primitives::Address, Vec<u8>> {
        Ok(self.vault.get())
    }

    pub fn sweep(
        &mut self,
        token: stylus_sdk::alloy_primitives::Address,
        to: stylus_sdk::alloy_primitives::Address,
        amount: stylus_sdk::alloy_primitives::U256,
    ) -> Result<(), Vec<u8>> {
        if self.vm().msg_sender() != self.vault.get() {
            return Err(alloc::format!("OnlyVault").into_bytes());
        }
        if amount == stylus_sdk::alloy_primitives::U256::ZERO {
            return Ok(());
        }
        let erc20 = IERC20::new(token);
        let ctx = Call::new_mutating(self);
        erc20
            .transfer(self.vm(), ctx, to, amount)
            .map_err(|_| alloc::format!("TransferFailed").into_bytes())?;
        Ok(())
    }
}
