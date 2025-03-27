use crate::{Address, Digest, PublicKey, Signature, Signer, ToDigest};
use anyhow::Result;
use parity_scale_codec::{Decode, Encode};

pub struct ContractSigner {
    signer: Signer,
    contract_address: Address,
}

impl ContractSigner {
    pub fn new(signer: Signer, contract_address: Address) -> Self {
        Self {
            signer,
            contract_address,
        }
    }

    pub fn sign_digest(&self, public_key: PublicKey, digest: Digest) -> Result<ContractSignature> {
        self.signer
            .sign_digest(
                public_key,
                to_contract_digest(digest, self.contract_address),
            )
            .map(|signature| ContractSignature { signature })
    }

    pub fn sign_data<T: ToDigest>(
        &self,
        public_key: PublicKey,
        data: &T,
    ) -> Result<ContractSignature> {
        self.sign_digest(public_key, data.to_digest())
    }

    pub fn contract_address(&self) -> Address {
        self.contract_address
    }
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct ContractSignature {
    signature: Signature,
}

impl ContractSignature {
    pub fn recover(&self, contract_address: Address, digest: Digest) -> Result<PublicKey> {
        self.signature
            .recover_from_digest(to_contract_digest(digest, contract_address))
    }

    pub fn verify_address(
        &self,
        contract_address: Address,
        address: Address,
        digest: Digest,
    ) -> Result<()> {
        if self.recover(contract_address, digest)?.to_address() != address {
            anyhow::bail!("Invalid signature");
        }

        Ok(())
    }
}

impl AsRef<[u8]> for ContractSignature {
    fn as_ref(&self) -> &[u8] {
        self.signature.as_ref()
    }
}

fn to_contract_digest(commitments_digest: Digest, contract_address: Address) -> Digest {
    // See explanation: https://eips.ethereum.org/EIPS/eip-191
    [
        [0x19, 0x00].as_ref(),
        contract_address.0.as_ref(),
        commitments_digest.as_ref(),
    ]
    .concat()
    .to_digest()
}
