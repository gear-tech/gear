// Copyright 2018-2019 Parity Technologies (UK) Ltd
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A voter in GRANDPA. This transitions between rounds and casts votes.
//!
//! Voters rely on some external context to function:
//!   - setting timers to cast votes.
//!   - incoming vote streams.
//!   - providing voter weights.
//!   - getting the local voter id.
//!
//!  The local voter id is used to check whether to cast votes for a given
//!  round. If no local id is defined or if it's not part of the voter set then
//!  votes will not be pushed to the sink. The protocol state machine still
//!  transitions state as if the votes had been pushed out.

use futures::{
    channel::mpsc::{self, UnboundedReceiver},
    prelude::*,
    ready,
};
#[cfg(feature = "std")]
use log::trace;

use parking_lot::Mutex;

use std::{
    collections::VecDeque,
    hash::Hash,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crate::{
    round::State as RoundState, validate_commit, voter_set::VoterSet, weights::VoteWeight,
    BlockNumberOps, CatchUp, Chain, Commit, CommitValidationResult, CompactCommit, Equivocation,
    HistoricalVotes, Message, Precommit, Prevote, PrimaryPropose, SignedMessage, LOG_TARGET,
};
use past_rounds::PastRounds;
use voting_round::{State as VotingRoundState, VotingRound};

mod past_rounds;
mod voting_round;

/// Necessary environment for a voter.
///
/// This encapsulates the database and networking layers of the chain.
pub trait Environment<H: Eq, N: BlockNumberOps>: Chain<H, N> {
    /// Associated timer type for the environment. See also [`Self::round_data`] and
    /// [`Self::round_commit_timer`].
    type Timer: Future<Output = Result<(), Self::Error>> + Unpin;
    /// Associated future type for the environment used when asynchronously computing the
    /// best chain to vote on. See also [`Self::best_chain_containing`].
    type BestChain: Future<Output = Result<Option<(H, N)>, Self::Error>> + Send + Unpin;
    /// The associated Id for the Environment.
    type Id: Clone + Eq + Ord + std::fmt::Debug;
    /// The associated Signature type for the Environment.
    type Signature: Eq + Clone;
    /// The input stream used to communicate with the outside world.
    type In: Stream<Item = Result<SignedMessage<H, N, Self::Signature, Self::Id>, Self::Error>>
        + Unpin;
    /// The output stream used to communicate with the outside world.
    type Out: Sink<Message<H, N>, Error = Self::Error> + Unpin;
    /// The associated Error type.
    type Error: From<crate::Error> + ::std::error::Error;

    /// Return a future that will resolve to the hash of the best block whose chain
    /// contains the given block hash, even if that block is `base` itself.
    ///
    /// If `base` is unknown the future outputs `None`.
    fn best_chain_containing(&self, base: H) -> Self::BestChain;

    /// Produce data necessary to start a round of voting. This may also be called
    /// with the round number of the most recently completed round, in which case
    /// it should yield a valid input stream.
    ///
    /// The input stream should provide messages which correspond to known blocks
    /// only.
    ///
    /// The voting logic will push unsigned messages over-eagerly into the
    /// output stream. It is the job of this stream to determine if those messages
    /// should be sent (for example, if the process actually controls a permissioned key)
    /// and then to sign the message, multicast it to peers, and schedule it to be
    /// returned by the `In` stream.
    ///
    /// This allows the voting logic to maintain the invariant that only incoming messages
    /// may alter the state, and the logic remains the same regardless of whether a node
    /// is a regular voter, the proposer, or simply an observer.
    ///
    /// Furthermore, this means that actual logic of creating and verifying
    /// signatures is flexible and can be maintained outside this crate.
    fn round_data(&self, round: u64) -> RoundData<Self::Id, Self::Timer, Self::In, Self::Out>;

    /// Return a timer that will be used to delay the broadcast of a commit
    /// message. This delay should not be static to minimize the amount of
    /// commit messages that are sent (e.g. random value in [0, 1] seconds).
    fn round_commit_timer(&self) -> Self::Timer;

    /// Note that we've done a primary proposal in the given round.
    fn proposed(&self, round: u64, propose: PrimaryPropose<H, N>) -> Result<(), Self::Error>;

    /// Note that we have prevoted in the given round.
    fn prevoted(&self, round: u64, prevote: Prevote<H, N>) -> Result<(), Self::Error>;

    /// Note that we have precommitted in the given round.
    fn precommitted(&self, round: u64, precommit: Precommit<H, N>) -> Result<(), Self::Error>;

    /// Note that a round is completed. This is called when a round has been
    /// voted in and the next round can start. The round may continue to be run
    /// in the background until _concluded_.
    /// Should return an error when something fatal occurs.
    fn completed(
        &self,
        round: u64,
        state: RoundState<H, N>,
        base: (H, N),
        votes: &HistoricalVotes<H, N, Self::Signature, Self::Id>,
    ) -> Result<(), Self::Error>;

    /// Note that a round has concluded. This is called when a round has been
    /// `completed` and additionally, the round's estimate has been finalized.
    ///
    /// There may be more votes than when `completed`, and it is the responsibility
    /// of the `Environment` implementation to deduplicate. However, the caller guarantees
    /// that the votes passed to `completed` for this round are a prefix of the votes passed here.
    fn concluded(
        &self,
        round: u64,
        state: RoundState<H, N>,
        base: (H, N),
        votes: &HistoricalVotes<H, N, Self::Signature, Self::Id>,
    ) -> Result<(), Self::Error>;

    /// Called when a block should be finalized.
    // TODO: make this a future that resolves when it's e.g. written to disk?
    fn finalize_block(
        &self,
        hash: H,
        number: N,
        round: u64,
        commit: Commit<H, N, Self::Signature, Self::Id>,
    ) -> Result<(), Self::Error>;

    /// Note that an equivocation in prevotes has occurred.
    fn prevote_equivocation(
        &self,
        round: u64,
        equivocation: Equivocation<Self::Id, Prevote<H, N>, Self::Signature>,
    );
    /// Note that an equivocation in precommits has occurred.
    fn precommit_equivocation(
        &self,
        round: u64,
        equivocation: Equivocation<Self::Id, Precommit<H, N>, Self::Signature>,
    );
}

/// Communication between nodes that is not round-localized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunicationOut<H, N, S, Id> {
    /// A commit message.
    Commit(u64, Commit<H, N, S, Id>),
}

/// The outcome of processing a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitProcessingOutcome {
    /// It was beneficial to process this commit.
    Good(GoodCommit),
    /// It wasn't beneficial to process this commit. We wasted resources.
    Bad(BadCommit),
}

#[cfg(any(test, feature = "test-helpers"))]
impl CommitProcessingOutcome {
    /// Returns a `Good` instance of commit processing outcome's opaque type. Useful for testing.
    pub fn good() -> CommitProcessingOutcome {
        CommitProcessingOutcome::Good(GoodCommit::new())
    }

    /// Returns a `Bad` instance of commit processing outcome's opaque type. Useful for testing.
    pub fn bad() -> CommitProcessingOutcome {
        CommitProcessingOutcome::Bad(CommitValidationResult::default().into())
    }
}

/// The result of processing for a good commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodCommit {
    _priv: (), // lets us add stuff without breaking API.
}

impl GoodCommit {
    pub(crate) fn new() -> Self {
        GoodCommit { _priv: () }
    }
}

/// The result of processing for a bad commit
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadCommit {
    _priv: (), // lets us add stuff without breaking API.
    num_precommits: usize,
    num_duplicated_precommits: usize,
    num_equivocations: usize,
    num_invalid_voters: usize,
}

impl BadCommit {
    /// Get the number of precommits
    pub fn num_precommits(&self) -> usize {
        self.num_precommits
    }

    /// Get the number of duplicated precommits
    pub fn num_duplicated(&self) -> usize {
        self.num_duplicated_precommits
    }

    /// Get the number of equivocations in the precommits
    pub fn num_equivocations(&self) -> usize {
        self.num_equivocations
    }

    /// Get the number of invalid voters in the precommits
    pub fn num_invalid_voters(&self) -> usize {
        self.num_invalid_voters
    }
}

impl From<CommitValidationResult> for BadCommit {
    fn from(r: CommitValidationResult) -> Self {
        BadCommit {
            num_precommits: r.num_precommits,
            num_duplicated_precommits: r.num_duplicated_precommits,
            num_equivocations: r.num_equivocations,
            num_invalid_voters: r.num_invalid_voters,
            _priv: (),
        }
    }
}

/// The outcome of processing a catch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatchUpProcessingOutcome {
    /// It was beneficial to process this catch up.
    Good(GoodCatchUp),
    /// It wasn't beneficial to process this catch up, it is invalid and we
    /// wasted resources.
    Bad(BadCatchUp),
    /// The catch up wasn't processed because it is useless, e.g. it is for a
    /// round lower than we're currently in.
    Useless,
}

#[cfg(any(test, feature = "test-helpers"))]
impl CatchUpProcessingOutcome {
    /// Returns a `Bad` instance of catch up processing outcome's opaque type. Useful for testing.
    pub fn bad() -> CatchUpProcessingOutcome {
        CatchUpProcessingOutcome::Bad(BadCatchUp::new())
    }

    /// Returns a `Good` instance of catch up processing outcome's opaque type. Useful for testing.
    pub fn good() -> CatchUpProcessingOutcome {
        CatchUpProcessingOutcome::Good(GoodCatchUp::new())
    }
}

/// The result of processing for a good catch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodCatchUp {
    _priv: (), // lets us add stuff without breaking API.
}

impl GoodCatchUp {
    pub(crate) fn new() -> Self {
        GoodCatchUp { _priv: () }
    }
}

/// The result of processing for a bad catch up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadCatchUp {
    _priv: (), // lets us add stuff without breaking API.
}

impl BadCatchUp {
    pub(crate) fn new() -> Self {
        BadCatchUp { _priv: () }
    }
}

/// Callback used to pass information about the outcome of importing a given
/// message (e.g. vote, commit, catch up). Useful to propagate data to the
/// network after making sure the import is successful.
pub enum Callback<O> {
    /// Default value.
    Blank,
    /// Callback to execute given a processing outcome.
    Work(Box<dyn FnMut(O) + Send>),
}

#[cfg(any(test, feature = "test-helpers"))]
impl<O> Clone for Callback<O> {
    fn clone(&self) -> Self {
        Callback::Blank
    }
}

impl<O> Callback<O> {
    /// Do the work associated with the callback, if any.
    pub fn run(&mut self, o: O) {
        match self {
            Callback::Blank => {}
            Callback::Work(cb) => cb(o),
        }
    }
}

/// Communication between nodes that is not round-localized.
#[cfg_attr(any(test, feature = "test-helpers"), derive(Clone))]
pub enum CommunicationIn<H, N, S, Id> {
    /// A commit message.
    Commit(
        u64,
        CompactCommit<H, N, S, Id>,
        Callback<CommitProcessingOutcome>,
    ),
    /// A catch up message.
    CatchUp(CatchUp<H, N, S, Id>, Callback<CatchUpProcessingOutcome>),
}

impl<H, N, S, Id> Unpin for CommunicationIn<H, N, S, Id> {}

/// Data necessary to participate in a round.
pub struct RoundData<Id, Timer, Input, Output> {
    /// Local voter id (if any.)
    pub voter_id: Option<Id>,
    /// Timer before prevotes can be cast. This should be Start + 2T
    /// where T is the gossip time estimate.
    pub prevote_timer: Timer,
    /// Timer before precommits can be cast. This should be Start + 4T
    pub precommit_timer: Timer,
    /// Incoming messages.
    pub incoming: Input,
    /// Outgoing messages.
    pub outgoing: Output,
}

struct Buffered<S, I> {
    inner: S,
    buffer: VecDeque<I>,
}

impl<S: Sink<I> + Unpin, I> Buffered<S, I> {
    fn new(inner: S) -> Buffered<S, I> {
        Buffered {
            buffer: VecDeque::new(),
            inner,
        }
    }

    // push an item into the buffered sink.
    // the sink _must_ be driven to completion with `poll` afterwards.
    fn push(&mut self, item: I) {
        self.buffer.push_back(item);
    }

    // returns ready when the sink and the buffer are completely flushed.
    fn poll(&mut self, cx: &mut Context) -> Poll<Result<(), S::Error>> {
        let polled = self.schedule_all(cx)?;

        match polled {
            Poll::Ready(()) => Sink::poll_flush(Pin::new(&mut self.inner), cx),
            Poll::Pending => {
                ready!(Sink::poll_flush(Pin::new(&mut self.inner), cx))?;
                Poll::Pending
            }
        }
    }

    fn schedule_all(&mut self, cx: &mut Context) -> Poll<Result<(), S::Error>> {
        while !self.buffer.is_empty() {
            ready!(Sink::poll_ready(Pin::new(&mut self.inner), cx))?;

            let item = self
                .buffer
                .pop_front()
                .expect("we checked self.buffer.is_empty() just above; qed");
            Sink::start_send(Pin::new(&mut self.inner), item)?;
        }

        Poll::Ready(Ok(()))
    }
}

type FinalizedNotification<H, N, E> = (
    H,
    N,
    u64,
    Commit<H, N, <E as Environment<H, N>>::Signature, <E as Environment<H, N>>::Id>,
);

// Instantiates the given last round, to be backgrounded until its estimate is finalized.
//
// This round must be completable based on the passed votes (and if not, `None` will be returned),
// but it may be the case that there are some more votes to propagate in order to push
// the estimate backwards and conclude the round (i.e. finalize its estimate).
//
// may only be called with non-zero last round.
fn instantiate_last_round<H, N, E: Environment<H, N>>(
    voters: VoterSet<E::Id>,
    last_round_votes: Vec<SignedMessage<H, N, E::Signature, E::Id>>,
    last_round_number: u64,
    last_round_base: (H, N),
    finalized_sender: mpsc::UnboundedSender<FinalizedNotification<H, N, E>>,
    env: Arc<E>,
) -> Option<VotingRound<H, N, E>>
where
    H: Clone + Eq + Ord + ::std::fmt::Debug,
    N: Copy + BlockNumberOps + ::std::fmt::Debug,
{
    let last_round_tracker = crate::round::Round::new(crate::round::RoundParams {
        voters,
        base: last_round_base,
        round_number: last_round_number,
    });

    // start as completed so we don't cast votes.
    let mut last_round = VotingRound::completed(last_round_tracker, finalized_sender, None, env);

    for vote in last_round_votes {
        // bail if any votes are bad.
        last_round.handle_vote(vote).ok()?;
    }

    if last_round.round_state().completable {
        Some(last_round)
    } else {
        None
    }
}

// The inner state of a voter aggregating the currently running round state
// (i.e. best and background rounds). This state exists separately since it's
// useful to wrap in a `Arc<Mutex<_>>` for sharing.
struct InnerVoterState<H, N, E>
where
    H: Clone + Ord + std::fmt::Debug,
    N: BlockNumberOps,
    E: Environment<H, N>,
{
    best_round: VotingRound<H, N, E>,
    past_rounds: PastRounds<H, N, E>,
}

/// A future that maintains and multiplexes between different rounds,
/// and caches votes.
///
/// This voter also implements the commit protocol.
/// The commit protocol allows a node to broadcast a message that finalizes a
/// given block and includes a set of precommits as proof.
///
/// - When a round is completable and we precommitted we start a commit timer
///   and start accepting commit messages;
/// - When we receive a commit message if it targets a block higher than what
///   we've finalized we validate it and import its precommits if valid;
/// - When our commit timer triggers we check if we've received any commit
///   message for a block equal to what we've finalized, if we haven't then we
///   broadcast a commit.
///
/// Additionally, we also listen to commit messages from rounds that aren't
/// currently running, we validate the commit and dispatch a finalization
/// notification (if any) to the environment.
pub struct Voter<H, N, E: Environment<H, N>, GlobalIn, GlobalOut>
where
    H: Clone + Eq + Ord + ::std::fmt::Debug,
    N: Copy + BlockNumberOps + ::std::fmt::Debug,
    GlobalIn: Stream<Item = Result<CommunicationIn<H, N, E::Signature, E::Id>, E::Error>> + Unpin,
    GlobalOut: Sink<CommunicationOut<H, N, E::Signature, E::Id>, Error = E::Error> + Unpin,
{
    env: Arc<E>,
    voters: VoterSet<E::Id>,
    inner: Arc<Mutex<InnerVoterState<H, N, E>>>,
    finalized_notifications: UnboundedReceiver<FinalizedNotification<H, N, E>>,
    last_finalized_number: N,
    global_in: GlobalIn,
    global_out: Buffered<GlobalOut, CommunicationOut<H, N, E::Signature, E::Id>>,
    // the commit protocol might finalize further than the current round (if we're
    // behind), we keep track of last finalized in round so we don't violate any
    // assumptions from round-to-round.
    last_finalized_in_rounds: (H, N),
}

impl<'a, H: 'a, N, E: 'a, GlobalIn, GlobalOut> Voter<H, N, E, GlobalIn, GlobalOut>
where
    H: Clone + Ord + ::std::fmt::Debug + Sync + Send,
    N: BlockNumberOps + Sync + Send,
    E: Environment<H, N> + Sync + Send,
    GlobalIn: Stream<Item = Result<CommunicationIn<H, N, E::Signature, E::Id>, E::Error>> + Unpin,
    GlobalOut: Sink<CommunicationOut<H, N, E::Signature, E::Id>, Error = E::Error> + Unpin,
{
    /// Returns an object allowing to query the voter state.
    pub fn voter_state(&self) -> Box<dyn VoterState<E::Id> + 'a + Send + Sync>
    where
        <E as Environment<H, N>>::Signature: Send,
        <E as Environment<H, N>>::Id: Hash + Send,
        <E as Environment<H, N>>::Timer: Send,
        <E as Environment<H, N>>::Out: Send,
        <E as Environment<H, N>>::In: Send,
    {
        Box::new(SharedVoterState(self.inner.clone()))
    }
}

impl<H, N, E: Environment<H, N>, GlobalIn, GlobalOut> Voter<H, N, E, GlobalIn, GlobalOut>
where
    H: Clone + Eq + Ord + ::std::fmt::Debug,
    N: Copy + BlockNumberOps + ::std::fmt::Debug,
    GlobalIn: Stream<Item = Result<CommunicationIn<H, N, E::Signature, E::Id>, E::Error>> + Unpin,
    GlobalOut: Sink<CommunicationOut<H, N, E::Signature, E::Id>, Error = E::Error> + Unpin,
{
    /// Create new `Voter` tracker with given round number and base block.
    ///
    /// Provide data about the last completed round. If there is no
    /// known last completed round, the genesis state (round number 0, no votes, genesis base),
    /// should be provided. When available, all messages required to complete
    /// the last round should be provided.
    ///
    /// The input stream for commit messages should provide commits which
    /// correspond to known blocks only (including all its precommits). It
    /// is also responsible for validating the signature data in commit
    /// messages.
    pub fn new(
        env: Arc<E>,
        voters: VoterSet<E::Id>,
        global_comms: (GlobalIn, GlobalOut),
        last_round_number: u64,
        last_round_votes: Vec<SignedMessage<H, N, E::Signature, E::Id>>,
        last_round_base: (H, N),
        last_finalized: (H, N),
    ) -> Self {
        let (finalized_sender, finalized_notifications) = mpsc::unbounded();
        let last_finalized_number = last_finalized.1;

        // re-start the last round and queue all messages to be processed on first poll.
        // keep it in the background so we can push the estimate backwards until finalized
        // by actually waiting for more messages.
        let mut past_rounds = PastRounds::new();
        let mut last_round_state =
            crate::bridge_state::bridge_state(RoundState::genesis(last_round_base.clone())).1;

        if last_round_number > 0 {
            let maybe_completed_last_round = instantiate_last_round(
                voters.clone(),
                last_round_votes,
                last_round_number,
                last_round_base,
                finalized_sender.clone(),
                env.clone(),
            );

            if let Some(mut last_round) = maybe_completed_last_round {
                last_round_state = last_round.bridge_state();
                past_rounds.push(&*env, last_round);
            }

            // when there is no information about the last completed round,
            // the best we can do is assume that the estimate == the given base
            // and that it is finalized. This is always the case for the genesis
            // round of a set.
        }

        let best_round = VotingRound::new(
            last_round_number + 1,
            voters.clone(),
            last_finalized.clone(),
            Some(last_round_state),
            finalized_sender,
            env.clone(),
        );

        let (global_in, global_out) = global_comms;

        let inner = Arc::new(Mutex::new(InnerVoterState {
            best_round,
            past_rounds,
        }));

        Voter {
            env,
            voters,
            inner,
            finalized_notifications,
            last_finalized_number,
            last_finalized_in_rounds: last_finalized,
            global_in,
            global_out: Buffered::new(global_out),
        }
    }

    fn prune_background_rounds(&mut self, cx: &mut Context) -> Result<(), E::Error> {
        {
            let mut inner = self.inner.lock();

            // Do work on all background rounds, broadcasting any commits generated.
            while let Poll::Ready(Some(item)) =
                Stream::poll_next(Pin::new(&mut inner.past_rounds), cx)
            {
                let (number, commit) = item?;
                self.global_out
                    .push(CommunicationOut::Commit(number, commit));
            }
        }

        while let Poll::Ready(res) =
            Stream::poll_next(Pin::new(&mut self.finalized_notifications), cx)
        {
            let inner = self.inner.clone();
            let mut inner = inner.lock();

            let (f_hash, f_num, round, commit) =
                res.expect("one sender always kept alive in self.best_round; qed");

            inner.past_rounds.update_finalized(f_num);

            if self.set_last_finalized_number(f_num) {
                self.env
                    .finalize_block(f_hash.clone(), f_num, round, commit)?;
            }

            if f_num > self.last_finalized_in_rounds.1 {
                self.last_finalized_in_rounds = (f_hash, f_num);
            }
        }

        Ok(())
    }

    /// Process all incoming messages from other nodes.
    ///
    /// Commit messages are handled with extra care. If a commit message references
    /// a currently backgrounded round, we send it to that round so that when we commit
    /// on that round, our commit message will be informed by those that we've seen.
    ///
    /// Otherwise, we will simply handle the commit and issue a finalization command
    /// to the environment.
    fn process_incoming(&mut self, cx: &mut Context) -> Result<(), E::Error> {
        while let Poll::Ready(Some(item)) = Stream::poll_next(Pin::new(&mut self.global_in), cx) {
            match item? {
                CommunicationIn::Commit(round_number, commit, mut process_commit_outcome) => {
                    trace!(
                        target: LOG_TARGET,
                        "Got commit for round_number {:?}: target_number: {:?}, target_hash: {:?}",
                        round_number,
                        commit.target_number,
                        commit.target_hash,
                    );

                    let commit: Commit<_, _, _, _> = commit.into();

                    let mut inner = self.inner.lock();

                    // if the commit is for a background round dispatch to round committer.
                    // that returns Some if there wasn't one.
                    if let Some(commit) = inner.past_rounds.import_commit(round_number, commit) {
                        // otherwise validate the commit and signal the finalized block from the
                        // commit to the environment (if valid and higher than current finalized)
                        let validation_result = validate_commit(&commit, &self.voters, &*self.env)?;

                        if validation_result.is_valid() {
                            // this can't be moved to a function because the compiler
                            // will complain about getting two mutable borrows to self
                            // (due to the call to `self.rounds.get_mut`).
                            let last_finalized_number = &mut self.last_finalized_number;

                            // clean up any background rounds
                            inner.past_rounds.update_finalized(commit.target_number);

                            if commit.target_number > *last_finalized_number {
                                *last_finalized_number = commit.target_number;
                                self.env.finalize_block(
                                    commit.target_hash.clone(),
                                    commit.target_number,
                                    round_number,
                                    commit,
                                )?;
                            }

                            process_commit_outcome
                                .run(CommitProcessingOutcome::Good(GoodCommit::new()));
                        } else {
                            // Failing validation of a commit is bad.
                            process_commit_outcome.run(CommitProcessingOutcome::Bad(
                                BadCommit::from(validation_result),
                            ));
                        }
                    } else {
                        // Import to backgrounded round is good.
                        process_commit_outcome
                            .run(CommitProcessingOutcome::Good(GoodCommit::new()));
                    }
                }
                CommunicationIn::CatchUp(catch_up, mut process_catch_up_outcome) => {
                    trace!(
                        target: LOG_TARGET,
                        "Got catch-up message for round {}",
                        catch_up.round_number
                    );

                    let mut inner = self.inner.lock();

                    let round = if let Some(round) = validate_catch_up(
                        catch_up,
                        &*self.env,
                        &self.voters,
                        inner.best_round.round_number(),
                    ) {
                        round
                    } else {
                        process_catch_up_outcome
                            .run(CatchUpProcessingOutcome::Bad(BadCatchUp::new()));
                        return Ok(());
                    };

                    let state = round.state();

                    // beyond this point, we set this round to the past and
                    // start voting in the next round.
                    let mut just_completed = VotingRound::completed(
                        round,
                        inner.best_round.finalized_sender(),
                        None,
                        self.env.clone(),
                    );

                    let new_best = VotingRound::new(
                        just_completed.round_number() + 1,
                        self.voters.clone(),
                        self.last_finalized_in_rounds.clone(),
                        Some(just_completed.bridge_state()),
                        inner.best_round.finalized_sender(),
                        self.env.clone(),
                    );

                    // update last-finalized in rounds _after_ starting new round.
                    // otherwise the base could be too eagerly set forward.
                    if let Some((f_hash, f_num)) = state.finalized.clone() {
                        if f_num > self.last_finalized_in_rounds.1 {
                            self.last_finalized_in_rounds = (f_hash, f_num);
                        }
                    }

                    self.env.completed(
                        just_completed.round_number(),
                        just_completed.round_state(),
                        just_completed.dag_base(),
                        just_completed.historical_votes(),
                    )?;

                    inner.past_rounds.push(&*self.env, just_completed);

                    let old_best = std::mem::replace(&mut inner.best_round, new_best);
                    inner.past_rounds.push(&*self.env, old_best);

                    process_catch_up_outcome
                        .run(CatchUpProcessingOutcome::Good(GoodCatchUp::new()));
                }
            }
        }

        Ok(())
    }

    // process the logic of the best round.
    fn process_best_round(&mut self, cx: &mut Context) -> Poll<Result<(), E::Error>> {
        // If the current `best_round` is completable and we've already precommitted,
        // we start a new round at `best_round + 1`.
        {
            let mut inner = self.inner.lock();

            let should_start_next = {
                let completable = match inner.best_round.poll(cx)? {
                    Poll::Ready(()) => true,
                    Poll::Pending => false,
                };

                // start when we've cast all votes.
                let precommitted = matches!(
                    inner.best_round.state(),
                    Some(&VotingRoundState::Precommitted)
                );

                completable && precommitted
            };

            if !should_start_next {
                return Poll::Pending;
            }

            trace!(
                target: LOG_TARGET,
                "Best round at {} has become completable. Starting new best round at {}",
                inner.best_round.round_number(),
                inner.best_round.round_number() + 1,
            );
        }

        self.completed_best_round()?;

        // round has been updated. so we need to re-poll.
        self.poll_unpin(cx)
    }

    fn completed_best_round(&mut self) -> Result<(), E::Error> {
        let mut inner = self.inner.lock();

        self.env.completed(
            inner.best_round.round_number(),
            inner.best_round.round_state(),
            inner.best_round.dag_base(),
            inner.best_round.historical_votes(),
        )?;

        let old_round_number = inner.best_round.round_number();

        let next_round = VotingRound::new(
            old_round_number + 1,
            self.voters.clone(),
            self.last_finalized_in_rounds.clone(),
            Some(inner.best_round.bridge_state()),
            inner.best_round.finalized_sender(),
            self.env.clone(),
        );

        let old_round = ::std::mem::replace(&mut inner.best_round, next_round);
        inner.past_rounds.push(&*self.env, old_round);
        Ok(())
    }

    fn set_last_finalized_number(&mut self, finalized_number: N) -> bool {
        let last_finalized_number = &mut self.last_finalized_number;
        if finalized_number > *last_finalized_number {
            *last_finalized_number = finalized_number;
            return true;
        }
        false
    }
}

impl<H, N, E: Environment<H, N>, GlobalIn, GlobalOut> Future for Voter<H, N, E, GlobalIn, GlobalOut>
where
    H: Clone + Eq + Ord + ::std::fmt::Debug,
    N: Copy + BlockNumberOps + ::std::fmt::Debug,
    GlobalIn: Stream<Item = Result<CommunicationIn<H, N, E::Signature, E::Id>, E::Error>> + Unpin,
    GlobalOut: Sink<CommunicationOut<H, N, E::Signature, E::Id>, Error = E::Error> + Unpin,
{
    type Output = Result<(), E::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), E::Error>> {
        self.process_incoming(cx)?;
        self.prune_background_rounds(cx)?;
        let _ = self.global_out.poll(cx)?;

        self.process_best_round(cx)
    }
}

impl<H, N, E: Environment<H, N>, GlobalIn, GlobalOut> Unpin for Voter<H, N, E, GlobalIn, GlobalOut>
where
    H: Clone + Eq + Ord + ::std::fmt::Debug,
    N: Copy + BlockNumberOps + ::std::fmt::Debug,
    GlobalIn: Stream<Item = Result<CommunicationIn<H, N, E::Signature, E::Id>, E::Error>> + Unpin,
    GlobalOut: Sink<CommunicationOut<H, N, E::Signature, E::Id>, Error = E::Error> + Unpin,
{
}

/// Trait for querying the state of the voter. Used by `Voter` to return a queryable object
/// without exposing too many data types.
pub trait VoterState<Id: Eq + std::hash::Hash> {
    /// Returns a plain data type, `report::VoterState`, describing the current state
    /// of the voter relevant to the voting process.
    fn get(&self) -> report::VoterState<Id>;
}

/// Contains a number of data transfer objects for reporting data to the outside world.
pub mod report {
    use crate::weights::{VoteWeight, VoterWeight};
    use std::collections::{HashMap, HashSet};

    /// Basic data struct for the state of a round.
    #[derive(PartialEq, Eq, Clone)]
    #[cfg_attr(test, derive(Debug))]
    pub struct RoundState<Id: Eq + std::hash::Hash> {
        /// Total weight of all votes.
        pub total_weight: VoterWeight,
        /// The threshold voter weight.
        pub threshold_weight: VoterWeight,

        /// Current weight of the prevotes.
        pub prevote_current_weight: VoteWeight,
        /// The identities of nodes that have cast prevotes so far.
        pub prevote_ids: HashSet<Id>,

        /// Current weight of the precommits.
        pub precommit_current_weight: VoteWeight,
        /// The identities of nodes that have cast precommits so far.
        pub precommit_ids: HashSet<Id>,
    }

    /// Basic data struct for the current state of the voter in a form suitable
    /// for passing on to other systems.
    #[derive(PartialEq, Eq)]
    #[cfg_attr(test, derive(Debug))]
    pub struct VoterState<Id: Eq + std::hash::Hash> {
        /// Voting rounds running in the background.
        pub background_rounds: HashMap<u64, RoundState<Id>>,
        /// The current best voting round.
        pub best_round: (u64, RoundState<Id>),
    }
}

struct SharedVoterState<H, N, E>(Arc<Mutex<InnerVoterState<H, N, E>>>)
where
    H: Clone + Ord + std::fmt::Debug,
    N: BlockNumberOps,
    E: Environment<H, N>;

impl<H, N, E> VoterState<E::Id> for SharedVoterState<H, N, E>
where
    H: Clone + Eq + Ord + std::fmt::Debug,
    N: BlockNumberOps,
    E: Environment<H, N>,
    <E as Environment<H, N>>::Id: Hash,
{
    fn get(&self) -> report::VoterState<E::Id> {
        let to_round_state = |voting_round: &VotingRound<H, N, E>| {
            (
                voting_round.round_number(),
                report::RoundState {
                    total_weight: voting_round.voters().total_weight(),
                    threshold_weight: voting_round.voters().threshold(),
                    prevote_current_weight: voting_round.prevote_weight(),
                    prevote_ids: voting_round.prevote_ids().collect(),
                    precommit_current_weight: voting_round.precommit_weight(),
                    precommit_ids: voting_round.precommit_ids().collect(),
                },
            )
        };

        let inner = self.0.lock();
        let best_round = to_round_state(&inner.best_round);
        let background_rounds = inner
            .past_rounds
            .voting_rounds()
            .map(to_round_state)
            .collect();

        report::VoterState {
            best_round,
            background_rounds,
        }
    }
}

/// Validate the given catch up and return a completed round with all prevotes
/// and precommits from the catch up imported. If the catch up is invalid `None`
/// is returned instead.
fn validate_catch_up<H, N, S, I, E>(
    catch_up: CatchUp<H, N, S, I>,
    env: &E,
    voters: &VoterSet<I>,
    best_round_number: u64,
) -> Option<crate::round::Round<I, H, N, S>>
where
    H: Clone + Eq + Ord + std::fmt::Debug,
    N: BlockNumberOps + std::fmt::Debug,
    S: Clone + Eq,
    I: Clone + Eq + std::fmt::Debug + Ord,
    E: Environment<H, N>,
{
    if catch_up.round_number <= best_round_number {
        trace!(target: LOG_TARGET, "Ignoring because best round number is {}", best_round_number);

        return None;
    }

    // check threshold support in prevotes and precommits.
    {
        let mut map = std::collections::BTreeMap::new();

        for prevote in &catch_up.prevotes {
            if !voters.contains(&prevote.id) {
                trace!(
                    target: LOG_TARGET,
                    "Ignoring invalid catch up, invalid voter: {:?}",
                    prevote.id,
                );

                return None;
            }

            map.entry(prevote.id.clone()).or_insert((false, false)).0 = true;
        }

        for precommit in &catch_up.precommits {
            if !voters.contains(&precommit.id) {
                trace!(
                    target: LOG_TARGET,
                    "Ignoring invalid catch up, invalid voter: {:?}",
                    precommit.id,
                );

                return None;
            }

            map.entry(precommit.id.clone()).or_insert((false, false)).1 = true;
        }

        let (pv, pc) = map.into_iter().fold(
            (VoteWeight(0), VoteWeight(0)),
            |(mut pv, mut pc), (id, (prevoted, precommitted))| {
                if let Some(v) = voters.get(&id) {
                    if prevoted {
                        pv = pv + v.weight();
                    }

                    if precommitted {
                        pc = pc + v.weight();
                    }
                }

                (pv, pc)
            },
        );

        let threshold = voters.threshold();
        if pv < threshold || pc < threshold {
            trace!(target: LOG_TARGET, "Ignoring invalid catch up, missing voter threshold");

            return None;
        }
    }

    let mut round = crate::round::Round::new(crate::round::RoundParams {
        round_number: catch_up.round_number,
        voters: voters.clone(),
        base: (catch_up.base_hash.clone(), catch_up.base_number),
    });

    // import prevotes first.
    for crate::SignedPrevote {
        prevote,
        id,
        signature,
    } in catch_up.prevotes
    {
        match round.import_prevote(env, prevote, id, signature) {
            Ok(_) => {}
            Err(e) => {
                trace!(
                    target: LOG_TARGET,
                    "Ignoring invalid catch up, error importing prevote: {:?}",
                    e,
                );

                return None;
            }
        }
    }

    // then precommits.
    for crate::SignedPrecommit {
        precommit,
        id,
        signature,
    } in catch_up.precommits
    {
        match round.import_precommit(env, precommit, id, signature) {
            Ok(_) => {}
            Err(e) => {
                trace!(
                    target: LOG_TARGET,
                    "Ignoring invalid catch up, error importing precommit: {:?}",
                    e,
                );

                return None;
            }
        }
    }

    let state = round.state();
    if !state.completable {
        return None;
    }

    Some(round)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        testing::{
            self,
            chain::GENESIS_HASH,
            environment::{Environment, Id, Signature},
        },
        weights::{VoteWeight, VoterWeight},
        SignedPrecommit,
    };
    use futures::{executor::LocalPool, task::SpawnExt};
    use futures_timer::Delay;
    use std::{collections::HashSet, iter, time::Duration};

    #[test]
    fn talking_to_myself() {
        let local_id = Id(5);
        let voters = VoterSet::new(std::iter::once((local_id, 100))).unwrap();

        let (network, routing_task) = testing::environment::make_network();

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network, local_id));

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        // run voter in background. scheduling it to shut down at the end.
        let finalized = env.finalized_stream();
        let voter = Voter::new(
            env.clone(),
            voters,
            global_comms,
            0,
            Vec::new(),
            last_finalized,
            last_finalized,
        );

        let mut pool = LocalPool::new();
        pool.spawner()
            .spawn(voter.map(|v| v.expect("Error voting")))
            .unwrap();
        pool.spawner().spawn(routing_task).unwrap();

        // wait for the best block to finalize.
        pool.run_until(
            finalized
                .take_while(|&(_, n, _)| future::ready(n < 6))
                .for_each(|_| future::ready(())),
        )
    }

    #[test]
    fn finalizing_at_fault_threshold() {
        // 10 voters
        let voters = VoterSet::new((0..10).map(|i| (Id(i), 1))).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();
        let mut pool = LocalPool::new();

        // 3 voters offline.
        let finalized_streams = (0..7)
            .map(|i| {
                let local_id = Id(i);
                // initialize chain
                let env = Arc::new(Environment::new(network.clone(), local_id));
                let last_finalized = env.with_chain(|chain| {
                    chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
                    chain.last_finalized()
                });

                // run voter in background. scheduling it to shut down at the end.
                let finalized = env.finalized_stream();
                let voter = Voter::new(
                    env.clone(),
                    voters.clone(),
                    network.make_global_comms(),
                    0,
                    Vec::new(),
                    last_finalized,
                    last_finalized,
                );

                pool.spawner()
                    .spawn(voter.map(|v| v.expect("Error voting")))
                    .unwrap();

                // wait for the best block to be finalized by all honest voters
                finalized
                    .take_while(|&(_, n, _)| future::ready(n < 6))
                    .for_each(|_| future::ready(()))
            })
            .collect::<Vec<_>>();

        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        pool.run_until(future::join_all(finalized_streams));
    }

    #[test]
    fn exposing_voter_state() {
        let num_voters = 10;
        let voters_online = 7;
        let voters = VoterSet::new((0..num_voters).map(|i| (Id(i), 1))).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();
        let mut pool = LocalPool::new();

        // some voters offline
        let (finalized_streams, voter_states): (Vec<_>, Vec<_>) = (0..voters_online)
            .map(|i| {
                let local_id = Id(i);
                // initialize chain
                let env = Arc::new(Environment::new(network.clone(), local_id));
                let last_finalized = env.with_chain(|chain| {
                    chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
                    chain.last_finalized()
                });

                // run voter in background. scheduling it to shut down at the end.
                let finalized = env.finalized_stream();
                let voter = Voter::new(
                    env.clone(),
                    voters.clone(),
                    network.make_global_comms(),
                    0,
                    Vec::new(),
                    last_finalized,
                    last_finalized,
                );
                let voter_state = voter.voter_state();

                pool.spawner()
                    .spawn(voter.map(|v| v.expect("Error voting")))
                    .unwrap();

                (
                    // wait for the best block to be finalized by all honest voters
                    finalized
                        .take_while(|&(_, n, _)| future::ready(n < 6))
                        .for_each(|_| future::ready(())),
                    voter_state,
                )
            })
            .unzip();

        let voter_state = &voter_states[0];
        voter_states.iter().all(|vs| vs.get() == voter_state.get());

        let expected_round_state = report::RoundState::<Id> {
            total_weight: VoterWeight::new(num_voters.into()).expect("nonzero"),
            threshold_weight: VoterWeight::new(voters_online.into()).expect("nonzero"),
            prevote_current_weight: VoteWeight(0),
            prevote_ids: Default::default(),
            precommit_current_weight: VoteWeight(0),
            precommit_ids: Default::default(),
        };

        assert_eq!(
            voter_state.get(),
            report::VoterState {
                background_rounds: Default::default(),
                best_round: (1, expected_round_state.clone()),
            }
        );

        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();
        pool.run_until(future::join_all(finalized_streams));

        assert_eq!(
            voter_state.get().best_round,
            (2, expected_round_state.clone())
        );
    }

    #[test]
    fn broadcast_commit() {
        let local_id = Id(5);
        let voters = VoterSet::new([(local_id, 100)].iter().cloned()).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();
        let (commits, _) = network.make_global_comms();

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network, local_id));

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        // run voter in background. scheduling it to shut down at the end.
        let voter = Voter::new(
            env.clone(),
            voters.clone(),
            global_comms,
            0,
            Vec::new(),
            last_finalized,
            last_finalized,
        );

        let mut pool = LocalPool::new();
        pool.spawner()
            .spawn(voter.map(|v| v.expect("Error voting")))
            .unwrap();
        pool.spawner().spawn(routing_task).unwrap();

        // wait for the node to broadcast a commit message
        pool.run_until(commits.take(1).for_each(|_| future::ready(())))
    }

    #[test]
    fn broadcast_commit_only_if_newer() {
        let local_id = Id(5);
        let test_id = Id(42);
        let voters =
            VoterSet::new([(local_id, 100), (test_id, 201)].iter().cloned()).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();
        let (commits_stream, commits_sink) = network.make_global_comms();
        let (round_stream, round_sink) = network.make_round_comms(1, test_id);

        let prevote = Message::Prevote(Prevote {
            target_hash: "E",
            target_number: 6,
        });

        let precommit = Message::Precommit(Precommit {
            target_hash: "E",
            target_number: 6,
        });

        let commit = (
            1,
            Commit {
                target_hash: "E",
                target_number: 6,
                precommits: vec![SignedPrecommit {
                    precommit: Precommit {
                        target_hash: "E",
                        target_number: 6,
                    },
                    signature: Signature(test_id.0),
                    id: test_id,
                }],
            },
        );

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network, local_id));

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        // run voter in background. scheduling it to shut down at the end.
        let voter = Voter::new(
            env.clone(),
            voters.clone(),
            global_comms,
            0,
            Vec::new(),
            last_finalized,
            last_finalized,
        );

        let mut pool = LocalPool::new();
        pool.spawner()
            .spawn(voter.map(|v| v.expect("Error voting: {:?}")))
            .unwrap();
        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        pool.spawner()
            .spawn(
                round_stream
                    .into_future()
                    .then(|(value, stream)| {
                        // wait for a prevote
                        assert!(match value {
                            Some(Ok(SignedMessage {
                                message: Message::Prevote(_),
                                id: Id(5),
                                ..
                            })) => true,
                            _ => false,
                        });
                        let votes = vec![prevote, precommit].into_iter().map(Result::Ok);
                        futures::stream::iter(votes)
                            .forward(round_sink)
                            .map(|_| stream) // send our prevote
                    })
                    .then(|stream| {
                        stream
                            .take_while(|value| match value {
                                // wait for a precommit
                                Ok(SignedMessage {
                                    message: Message::Precommit(_),
                                    id: Id(5),
                                    ..
                                }) => future::ready(false),
                                _ => future::ready(true),
                            })
                            .for_each(|_| future::ready(()))
                    })
                    .then(move |_| {
                        // send our commit
                        stream::iter(iter::once(Ok(CommunicationOut::Commit(commit.0, commit.1))))
                            .forward(commits_sink)
                    })
                    .map(|_| ()),
            )
            .unwrap();

        let res = pool.run_until(
            // wait for the first commit (ours)
            commits_stream.into_future().then(|(_, stream)| {
                // the second commit should never arrive
                let await_second = stream.take(1).for_each(|_| future::ready(()));
                let delay = Delay::new(Duration::from_millis(500));
                future::select(await_second, delay)
            }),
        );

        match res {
            future::Either::Right(((), _work)) => {
                // the future timed out as expected
            }
            _ => panic!("Unexpected result"),
        }
    }

    #[test]
    fn import_commit_for_any_round() {
        let local_id = Id(5);
        let test_id = Id(42);
        let voters =
            VoterSet::new([(local_id, 100), (test_id, 201)].iter().cloned()).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();
        let (_, commits_sink) = network.make_global_comms();

        // this is a commit for a previous round
        let commit = Commit {
            target_hash: "E",
            target_number: 6,
            precommits: vec![SignedPrecommit {
                precommit: Precommit {
                    target_hash: "E",
                    target_number: 6,
                },
                signature: Signature(test_id.0),
                id: test_id,
            }],
        };

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network, local_id));

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        // run voter in background.
        let voter = Voter::new(
            env.clone(),
            voters.clone(),
            global_comms,
            1,
            Vec::new(),
            last_finalized,
            last_finalized,
        );

        let mut pool = LocalPool::new();
        pool.spawner()
            .spawn(voter.map(|v| v.expect("Error voting")))
            .unwrap();
        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        // Send the commit message.
        pool.spawner()
            .spawn(
                stream::iter(iter::once(Ok(CommunicationOut::Commit(0, commit.clone()))))
                    .forward(commits_sink)
                    .map(|_| ()),
            )
            .unwrap();

        // Wait for the commit message to be processed.
        let finalized = pool.run_until(
            env.finalized_stream()
                .into_future()
                .map(move |(msg, _)| msg.unwrap().2),
        );

        assert_eq!(finalized, commit);
    }

    #[test]
    fn skips_to_latest_round_after_catch_up() {
        // 3 voters
        let voters = VoterSet::new((0..3).map(|i| (Id(i), 1u64))).expect("nonempty");
        let total_weight = voters.total_weight();
        let threshold_weight = voters.threshold();
        let voter_ids: HashSet<Id> = (0..3).map(Id).collect();

        let (network, routing_task) = testing::environment::make_network();
        let mut pool = LocalPool::new();

        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        // initialize unsynced voter at round 0
        let (env, unsynced_voter) = {
            let local_id = Id(4);

            let env = Arc::new(Environment::new(network.clone(), local_id));
            let last_finalized = env.with_chain(|chain| {
                chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
                chain.last_finalized()
            });

            let voter = Voter::new(
                env.clone(),
                voters.clone(),
                network.make_global_comms(),
                0,
                Vec::new(),
                last_finalized,
                last_finalized,
            );

            (env, voter)
        };

        let pv = |id| crate::SignedPrevote {
            prevote: crate::Prevote {
                target_hash: "C",
                target_number: 4,
            },
            id: Id(id),
            signature: Signature(99),
        };

        let pc = |id| crate::SignedPrecommit {
            precommit: crate::Precommit {
                target_hash: "C",
                target_number: 4,
            },
            id: Id(id),
            signature: Signature(99),
        };

        // send in a catch-up message for round 5.
        network.send_message(CommunicationIn::CatchUp(
            CatchUp {
                base_number: 1,
                base_hash: GENESIS_HASH,
                round_number: 5,
                prevotes: vec![pv(0), pv(1), pv(2)],
                precommits: vec![pc(0), pc(1), pc(2)],
            },
            Callback::Blank,
        ));

        let voter_state = unsynced_voter.voter_state();
        assert_eq!(voter_state.get().background_rounds.get(&5), None);

        // spawn the voter in the background
        pool.spawner().spawn(unsynced_voter.map(|_| ())).unwrap();

        // wait until it's caught up, it should skip to round 6 and send a
        // finality notification for the block that was finalized by catching
        // up.
        let caught_up = future::poll_fn(|_| {
            if voter_state.get().best_round.0 == 6 {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        });

        let finalized = env.finalized_stream().take(1).into_future();

        pool.run_until(caught_up.then(|_| finalized.map(|_| ())));

        assert_eq!(
            voter_state.get().best_round,
            (
                6,
                report::RoundState::<Id> {
                    total_weight,
                    threshold_weight,
                    prevote_current_weight: VoteWeight(0),
                    prevote_ids: Default::default(),
                    precommit_current_weight: VoteWeight(0),
                    precommit_ids: Default::default(),
                }
            )
        );

        assert_eq!(
            voter_state.get().background_rounds.get(&5),
            Some(&report::RoundState::<Id> {
                total_weight,
                threshold_weight,
                prevote_current_weight: VoteWeight(3),
                prevote_ids: voter_ids.clone(),
                precommit_current_weight: VoteWeight(3),
                precommit_ids: voter_ids,
            })
        );
    }

    #[test]
    fn pick_up_from_prior_without_grandparent_state() {
        let local_id = Id(5);
        let voters = VoterSet::new(std::iter::once((local_id, 100))).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network, local_id));

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        // run voter in background. scheduling it to shut down at the end.
        let voter = Voter::new(
            env.clone(),
            voters,
            global_comms,
            10,
            Vec::new(),
            last_finalized,
            last_finalized,
        );

        let mut pool = LocalPool::new();
        pool.spawner()
            .spawn(voter.map(|v| v.expect("Error voting")))
            .unwrap();
        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        // wait for the best block to finalize.
        pool.run_until(
            env.finalized_stream()
                .take_while(|&(_, n, _)| future::ready(n < 6))
                .for_each(|_| future::ready(())),
        )
    }

    #[test]
    fn pick_up_from_prior_with_grandparent_state() {
        let local_id = Id(99);
        let voters = VoterSet::new((0..100).map(|i| (Id(i), 1))).expect("nonempty");

        let (network, routing_task) = testing::environment::make_network();

        let global_comms = network.make_global_comms();
        let env = Arc::new(Environment::new(network.clone(), local_id));
        let outer_env = env.clone();

        // initialize chain
        let last_finalized = env.with_chain(|chain| {
            chain.push_blocks(GENESIS_HASH, &["A", "B", "C", "D", "E"]);
            chain.last_finalized()
        });

        let mut pool = LocalPool::new();
        let mut last_round_votes = Vec::new();

        // round 1 state on disk: 67 prevotes for "E". 66 precommits for "D". 1 precommit "E".
        // the round is completable, but the estimate ("E") is not finalized.
        for id in 0..67 {
            let prevote = Message::Prevote(Prevote {
                target_hash: "E",
                target_number: 6,
            });
            let precommit = if id < 66 {
                Message::Precommit(Precommit {
                    target_hash: "D",
                    target_number: 5,
                })
            } else {
                Message::Precommit(Precommit {
                    target_hash: "E",
                    target_number: 6,
                })
            };

            last_round_votes.push(SignedMessage {
                message: prevote.clone(),
                signature: Signature(id),
                id: Id(id),
            });

            last_round_votes.push(SignedMessage {
                message: precommit.clone(),
                signature: Signature(id),
                id: Id(id),
            });

            // round 2 has the same votes.
            //
            // this means we wouldn't be able to start round 3 until
            // the estimate of round-1 moves backwards.
            let (_, round_sink) = network.make_round_comms(2, Id(id));
            let msgs = stream::iter(iter::once(Ok(prevote)).chain(iter::once(Ok(precommit))));
            pool.spawner()
                .spawn(msgs.forward(round_sink).map(|r| r.unwrap()))
                .unwrap();
        }

        // round 1 fresh communication. we send one more precommit for "D" so the estimate
        // moves backwards.
        let sender = Id(67);
        let (_, round_sink) = network.make_round_comms(1, sender);
        let last_precommit = Message::Precommit(Precommit {
            target_hash: "D",
            target_number: 3,
        });
        pool.spawner()
            .spawn(
                stream::iter(iter::once(Ok(last_precommit)))
                    .forward(round_sink)
                    .map(|r| r.unwrap()),
            )
            .unwrap();

        // run voter in background. scheduling it to shut down at the end.
        let voter = Voter::new(
            env.clone(),
            voters,
            global_comms,
            1,
            last_round_votes,
            last_finalized,
            last_finalized,
        );

        pool.spawner()
            .spawn(voter.map_err(|_| panic!("Error voting")).map(|_| ()))
            .unwrap();
        pool.spawner().spawn(routing_task.map(|_| ())).unwrap();

        // wait until we see a prevote on round 3 from our local ID,
        // indicating that the round 3 has started.

        let (round_stream, _) = network.make_round_comms(3, Id(1000));
        pool.run_until(
            round_stream
                .skip_while(move |v| {
                    let v = v.as_ref().unwrap();
                    if let Message::Prevote(_) = v.message {
                        future::ready(v.id != local_id)
                    } else {
                        future::ready(true)
                    }
                })
                .into_future()
                .map(|_| ()),
        );

        assert_eq!(outer_env.last_completed_and_concluded(), (2, 1));
    }
}
