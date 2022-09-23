use crate::{
    args::SeedVariant,
    generators,
    utils::{LoaderRng, LoaderRngCore},
};

// TODO DN
pub(super) struct TaskContextUpdate;

#[derive(Clone)]
pub(super) struct TasksContext {
    pub code_seed_gen: Box<dyn LoaderRngCore>,
    // TODO DN
    // pub programs: Vec<ProgramId>, // for send_message/send_reply
    // pub mailbox: Vec<Mailbox>, // for send_reply and claim_value
}

impl TasksContext {
    pub(super) fn new<Rng: LoaderRng>(code_seed_type: Option<SeedVariant>) -> Self {
        Self {
            code_seed_gen: generators::get_some_seed_generator::<Rng>(code_seed_type),
        }
    }

    pub(super) fn update(&mut self, _: TaskContextUpdate) {
        todo!("Todo DN")
    }
}
