use crate::utils::LoaderRng;

// TODO DN
pub(super) struct TaskContextUpdate;

#[derive(Clone)]
pub(super) struct TasksContext {
    // TODO DN
    // pub programs: Vec<ProgramId>, // for send_message/send_reply
    // pub mailbox: Vec<Mailbox>, // for send_reply and claim_value
}

impl TasksContext {
    pub(super) fn new<Rng: LoaderRng>() -> Self {
        Self {}
    }

    pub(super) fn update(&mut self, _: TaskContextUpdate) {
        todo!("Todo DN")
    }
}
