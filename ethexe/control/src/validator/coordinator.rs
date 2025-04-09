use anyhow::{anyhow, ensure, Result};
use ethexe_common::gear::BatchCommitment;
use ethexe_signer::Address;
use std::collections::BTreeSet;

use super::{submitter::Submitter, ValidatorContext, ValidatorSubService};
use crate::{
    utils::{BatchCommitmentValidationRequest, MultisignedBatchCommitment},
    BatchCommitmentValidationReply, ControlEvent,
};

pub struct Coordinator {
    ctx: ValidatorContext,
    validators: BTreeSet<Address>,
    multisigned_batch: MultisignedBatchCommitment,
}

impl ValidatorSubService for Coordinator {
    fn log(&self, s: String) -> String {
        format!("COORDINATOR - {s}")
    }

    fn to_dyn(self: Box<Self>) -> Box<dyn ValidatorSubService> {
        self
    }

    fn context(&self) -> &ValidatorContext {
        &self.ctx
    }

    fn context_mut(&mut self) -> &mut ValidatorContext {
        &mut self.ctx
    }

    fn into_context(self: Box<Self>) -> ValidatorContext {
        self.ctx
    }

    fn process_validation_reply(
        mut self: Box<Self>,
        reply: BatchCommitmentValidationReply,
    ) -> Result<Box<dyn ValidatorSubService>> {
        if let Err(err) = self
            .multisigned_batch
            .accept_batch_commitment_validation_reply(reply, |addr| {
                self.validators
                    .contains(&addr)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Received validation reply is not from known validator"))
            })
        {
            self.warning(format!("validation reply rejected: {err}"));
        }

        if self.multisigned_batch.signatures().len() as u64 >= self.ctx.threshold {
            Submitter::create(self.ctx, self.multisigned_batch)
        } else {
            Ok(self)
        }
    }
}

impl Coordinator {
    pub fn create(
        mut ctx: ValidatorContext,
        validators: Vec<Address>,
        batch: BatchCommitment,
    ) -> Result<Box<dyn ValidatorSubService>> {
        ensure!(
            validators.len() as u64 >= ctx.threshold,
            "Number of validators is less than threshold"
        );

        ensure!(ctx.threshold > 0, "Threshold should be greater than 0");

        let multisigned_batch = MultisignedBatchCommitment::new(
            batch,
            &ctx.signer.contract_signer(ctx.router_address),
            ctx.pub_key,
        )?;

        if multisigned_batch.signatures().len() as u64 >= ctx.threshold {
            return Submitter::create(ctx, multisigned_batch);
        }

        let validation_request = ctx.signer.create_signed_data(
            ctx.pub_key,
            BatchCommitmentValidationRequest::from(multisigned_batch.batch()),
        )?;

        ctx.output(ControlEvent::PublishValidationRequest(validation_request));

        Ok(Box::new(Self {
            ctx,
            validators: validators.into_iter().collect(),
            multisigned_batch,
        }))
    }
}
#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use ethexe_signer::ToDigest;
    use gprimitives::H256;

    use super::*;
    use crate::{tests::*, validator::tests::*};

    #[test]
    fn coordinator_create_success() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.threshold = 2;
        let validators: Vec<_> = keys.iter().take(3).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        let coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Coordinator>());
        assert!(matches!(
            coordinator.context().output[0],
            ControlEvent::PublishValidationRequest(_)
        ));
    }

    #[test]
    fn coordinator_create_insufficient_validators() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.threshold = 3;
        let validators = keys.iter().take(2).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators, batch).is_err(),
            "Expected an error, but got Ok"
        );
    }

    #[test]
    fn coordinator_create_zero_threshold() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.threshold = 0;
        let validators: Vec<_> = keys.iter().take(1).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();

        assert!(
            Coordinator::create(ctx, validators, batch).is_err(),
            "Expected an error due to zero threshold, but got Ok"
        );
    }

    #[test]
    fn process_validation_reply() {
        let (mut ctx, keys) = mock_validator_context();
        ctx.threshold = 3;
        let validators: Vec<_> = keys.iter().take(3).map(|k| k.to_address()).collect();
        let batch = BatchCommitment::default();
        let digest = batch.to_digest();

        let reply1 = mock_validation_reply(&ctx.signer, keys[0], ctx.router_address, digest);
        let reply2_invalid =
            mock_validation_reply(&ctx.signer, keys[4], ctx.router_address, digest);
        let reply3_invalid = mock_validation_reply(
            &ctx.signer,
            keys[1],
            ctx.router_address,
            H256::random().0.into(),
        );
        let reply4 = mock_validation_reply(&ctx.signer, keys[2], ctx.router_address, digest);

        let mut coordinator = Coordinator::create(ctx, validators, batch).unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Coordinator>());
        assert!(matches!(
            coordinator.context().output[0],
            ControlEvent::PublishValidationRequest(_)
        ));

        coordinator = coordinator.process_validation_reply(reply1).unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Coordinator>());

        coordinator = coordinator
            .process_validation_reply(reply2_invalid)
            .unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Coordinator>());
        assert!(matches!(
            coordinator.context().output[1],
            ControlEvent::Warning(_)
        ));

        coordinator = coordinator
            .process_validation_reply(reply3_invalid)
            .unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Coordinator>());
        assert!(matches!(
            coordinator.context().output[2],
            ControlEvent::Warning(_)
        ));

        coordinator = coordinator.process_validation_reply(reply4).unwrap();
        assert_eq!(coordinator.type_id(), TypeId::of::<Submitter>());
        assert_eq!(coordinator.context().output.len(), 3);
    }
}
