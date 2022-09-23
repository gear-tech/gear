//! Utils

use crate::{api::signer::Signer, result::Result};

impl Signer {
    /// Get self balance
    pub async fn balance(&self) -> Result<u128> {
        self.get_balance(&self.address()).await
    }

    /// Logging balance spent
    pub async fn log_balance_spent(&self, before: u128) -> Result<()> {
        let after = before.saturating_sub(self.balance().await?);
        log::info!("\tBalance spent: {after}");

        Ok(())
    }
}
