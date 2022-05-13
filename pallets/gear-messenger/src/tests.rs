use super::*;
use crate::mock::*;
use common::storage::{Messenger as Mgr, StorageDeque as SDq, *};
use gear_core::{
    ids::MessageId,
    message::{DispatchKind, StoredDispatch, StoredMessage},
};

pub(crate) fn init_logger() {
    let _ = env_logger::Builder::from_default_env()
        .format_module_path(false)
        .format_level(true)
        .try_init();
}

#[test]
fn sent_impl_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 0);

        // Bottom overflow check.
        <Pallet<Test> as Mgr>::Sent::decrease();

        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 0);

        // Increasing/decreasing check.
        <Pallet<Test> as Mgr>::Sent::increase();
        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 1);

        <Pallet<Test> as Mgr>::Sent::increase();
        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 2);

        <Pallet<Test> as Mgr>::Sent::decrease();

        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 1);

        // Clear check.
        <Pallet<Test> as Mgr>::Sent::clear();
        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 0);

        // Value updates for future blocks.
        <Pallet<Test> as Mgr>::Sent::increase();

        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 1);

        run_to_block(2);

        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 0);
    });
}

// Identical to `sent_impl_works` test, due to the same trait impl,
// but works on other storage and wont be actually manually used.
//
// See tests with auto incresing that parameter on pushes and pops from queue.
#[test]
fn dequeued_impl_works_manually() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);

        // Bottom overflow check.
        <Pallet<Test> as Mgr>::Dequeued::decrease();

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);

        // Increasing/decreasing check.
        <Pallet<Test> as Mgr>::Dequeued::increase();
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);

        <Pallet<Test> as Mgr>::Dequeued::increase();
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 2);

        <Pallet<Test> as Mgr>::Dequeued::decrease();

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);

        // Clear check.
        <Pallet<Test> as Mgr>::Dequeued::clear();
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);

        // Value updates for future blocks.
        <Pallet<Test> as Mgr>::Sent::increase();

        assert_eq!(<Pallet<Test> as Mgr>::Sent::get(), 1);

        run_to_block(2);

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
    });
}

// `QueueProcessing` won't be manually used.
//
// See tests with auto changing that parameter on pushes and pops from queue.
#[test]
fn queue_processing_impl_works_manually() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert!(<Pallet<Test> as Mgr>::QueueProcessing::allowed());

        // Invariants always.
        assert_ne!(
            <Pallet<Test> as Mgr>::QueueProcessing::allowed(),
            <Pallet<Test> as Mgr>::QueueProcessing::denied()
        );

        // Denying check.
        <Pallet<Test> as Mgr>::QueueProcessing::deny();
        assert!(!<Pallet<Test> as Mgr>::QueueProcessing::allowed());
        assert_ne!(
            <Pallet<Test> as Mgr>::QueueProcessing::allowed(),
            <Pallet<Test> as Mgr>::QueueProcessing::denied()
        );

        // Allowing check.
        <Pallet<Test> as Mgr>::QueueProcessing::allow();
        assert!(<Pallet<Test> as Mgr>::QueueProcessing::allowed());
        assert_ne!(
            <Pallet<Test> as Mgr>::QueueProcessing::allowed(),
            <Pallet<Test> as Mgr>::QueueProcessing::denied()
        );

        // Value updates for future blocks.
        <Pallet<Test> as Mgr>::QueueProcessing::deny();
        assert!(<Pallet<Test> as Mgr>::QueueProcessing::denied());

        run_to_block(2);

        assert!(!<Pallet<Test> as Mgr>::QueueProcessing::denied());
    });
}

#[test]
fn queue_works() {
    init_logger();
    new_test_ext().execute_with(|| {
        // Initial state of the block.
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 0);
        assert!(<Pallet<Test> as Mgr>::QueueProcessing::allowed());
        assert!(<<Pallet<Test> as Mgr>::Queue as SDq>::is_empty().expect("Algorithmic error"));

        // Dispatch constructor.
        let dispatch_with_id = |id: MessageId| {
            StoredDispatch::new(
                DispatchKind::Handle,
                StoredMessage::new(
                    id,
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                    Default::default(),
                ),
                None,
            )
        };

        // Ids for future manipulations.
        let id_1: MessageId = 1.into();
        let id_2: MessageId = 2.into();
        let id_3: MessageId = 3.into();
        let id_4: MessageId = 4.into();
        let id_5: MessageId = 5.into();

        // Pop from empty queue.
        assert!(<<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
            .expect("Algorithmic error")
            .is_none());

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 0);
        assert!(<<Pallet<Test> as Mgr>::Queue as SDq>::is_empty().expect("Algorithmic error"));

        // Push an element in empty queue.
        <<Pallet<Test> as Mgr>::Queue as SDq>::push_back(dispatch_with_id(id_1))
            .expect("Algorithmic error");
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 1);
        assert!(!<<Pallet<Test> as Mgr>::Queue as SDq>::is_empty().expect("Algorithmic error"));

        // Pop from single-element queue.
        assert_eq!(
            id_1,
            <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 0);
        assert!(<<Pallet<Test> as Mgr>::Queue as SDq>::is_empty().expect("Algorithmic error"));

        // Push back many elements in empty queue.
        <<Pallet<Test> as Mgr>::Queue as SDq>::push_back(dispatch_with_id(id_2))
            .expect("Algorithmic error");

        <<Pallet<Test> as Mgr>::Queue as SDq>::push_back(dispatch_with_id(id_3))
            .expect("Algorithmic error");

        <<Pallet<Test> as Mgr>::Queue as SDq>::push_back(dispatch_with_id(id_4))
            .expect("Algorithmic error");

        <<Pallet<Test> as Mgr>::Queue as SDq>::push_back(dispatch_with_id(id_5))
            .expect("Algorithmic error");

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 4);

        // Dequeued resets for future blocks.
        run_to_block(2);
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 4);

        // Pop 2 of 4 messages.
        assert_eq!(
            id_2,
            <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        let dispatch_3 = <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
            .expect("Algorithmic error")
            .expect("No dispatches found");

        assert_eq!(id_3, dispatch_3.id());

        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 2);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 2);

        // Push front used only for requeueing element,
        // which was already in queue in current block,
        // because it decreased dequeued amount.
        assert!(<Pallet<Test> as Mgr>::QueueProcessing::allowed());

        <<Pallet<Test> as Mgr>::Queue as SDq>::push_front(dispatch_3).expect("Algorithmic error");

        assert!(<Pallet<Test> as Mgr>::QueueProcessing::denied());
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 3);

        // Reset QueueProcessing deny.
        run_to_block(3);

        assert!(<Pallet<Test> as Mgr>::QueueProcessing::allowed());

        // Make the only one message be in queue.
        assert_eq!(
            id_3,
            <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        assert_eq!(
            id_4,
            <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
                .expect("Algorithmic error")
                .expect("No dispatches found")
                .id()
        );

        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 1);

        // Reset dequeued amount.
        run_to_block(4);

        // Push front works on queue with one element
        let dispatch_5 = <<Pallet<Test> as Mgr>::Queue as SDq>::pop_front()
            .expect("Algorithmic error")
            .expect("No dispatches found");

        assert_eq!(id_5, dispatch_5.id());

        assert!(<<Pallet<Test> as Mgr>::Queue as SDq>::is_empty().expect("Algorithmic error"));
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 1);

        <<Pallet<Test> as Mgr>::Queue as SDq>::push_front(dispatch_5).expect("Algorithmic error");

        assert!(<Pallet<Test> as Mgr>::QueueProcessing::denied());
        assert_eq!(<Pallet<Test> as Mgr>::Dequeued::get(), 0);
        assert_eq!(<<Pallet<Test> as Mgr>::Queue as SDq>::Length::get(), 1);
    });
}
