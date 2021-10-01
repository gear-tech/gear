#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
#[cfg(not(feature = "std"))]
use gstd::{prelude::*, *};

#[cfg(feature = "std")]
#[cfg(test)]
mod native {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Request {
    Empty,
    Spin,
    Panic,
    Allocate(u32),
    ResizeStatic(u32),
}

#[derive(Encode, Debug, Decode, PartialEq)]
pub enum Reply {
    Empty,
    Error,
}

#[cfg(not(feature = "std"))]
mod wasm {
    extern crate alloc;

    use codec::{Decode, Encode};
    use gstd::{ext, msg, prelude::*, MessageId, ProgramId};

    use super::{Reply, Request};

    static mut BUFFER: Vec<u8> = Vec::new();

    fn process_request(request: Request) -> Reply {
        match request {
            Request::Empty => Reply::Empty,
            Request::Spin => loop {},
            Request::Panic => panic!("Panic by request"),
            Request::Allocate(size) => {
                let _vec: Vec<u8> = Vec::with_capacity(size as usize);
                Reply::Empty
            }
            Request::ResizeStatic(size) => {
                unsafe { BUFFER = Vec::with_capacity(size as usize) };
                Reply::Empty
            }
        }
    }

    #[no_mangle]
    pub unsafe extern "C" fn init() {
        msg::load::<Request>()
            .map(process_request)
            .map(|_| ())
            .unwrap_or_else(|e| {
                msg::load::<()>()
                    .unwrap_or_else(|_| ext::debug(&format!("Error processing request: {:?}", e)))
            });

        msg::reply((), 0, 0);
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle() {
        let reply = msg::load::<Request>()
            .map(process_request)
            .unwrap_or_else(|e| {
                ext::debug(&format!("Error processing request: {:?}", e));
                Reply::Error
            });

        msg::reply(reply, 0, 0);
    }

    #[no_mangle]
    pub unsafe extern "C" fn handle_reply() {}
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::{native, Reply, Request};
    use common::*;
    use gear_core::memory::PAGE_SIZE;
    use gear_core_runner::Config;

    #[test]
    fn binary_available() {
        assert!(native::WASM_BINARY.is_some());
        assert!(native::WASM_BINARY_BLOATY.is_some());
    }

    fn wasm_code() -> &'static [u8] {
        native::WASM_BINARY_BLOATY.expect("wasm binary exists")
    }

    #[test]
    fn program_can_be_initialized() {
        let mut runner = RunnerContext::default();

        // Assertions are performed when decoding reply
        let _reply: () =
            runner.init_program_with_reply(InitProgram::from(wasm_code()).message(Request::Empty));
    }

    #[test]
    fn handle_spin_error() {
        let mut runner = RunnerContext::default();
        runner.init_program(InitProgram::from(wasm_code()).message(Request::Empty));

        let baseline: RunReport<Reply> =
            runner.request_report(MessageBuilder::from(Request::Empty));
        assert_eq!(baseline.result, RunResult::Normal);

        let report: RunReport<Reply> = runner
            .request_report(MessageBuilder::from(Request::Spin).gas_limit(10 * baseline.gas_spent));

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn init_spin_error() {
        let mut runner = RunnerContext::default();

        let baseline: RunReport<()> =
            runner.init_program_with_report(InitProgram::from(wasm_code()).message(Request::Empty));
        assert_eq!(baseline.result, RunResult::Normal);

        let report: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code())
                .message(MessageBuilder::from(Request::Spin).gas_limit(10 * baseline.gas_spent)),
        );

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn init_init_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);

        let baseline: RunReport<()> =
            runner.init_program_with_report(InitProgram::from(wasm_code()).message(Request::Empty));
        assert_eq!(baseline.result, RunResult::Normal);

        config.init_cost = 1000;

        // Init cost is charged for all static pages.
        let gas_limit = baseline.gas_spent + 17 * config.init_cost;

        let mut runner = RunnerContext::with_config(&config);
        let report: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code())
                .message(MessageBuilder::from(Request::Empty).gas_limit(gas_limit)),
        );

        assert_eq!(report.result, RunResult::Normal);

        let report: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code())
                .message(MessageBuilder::from(Request::Empty).gas_limit(gas_limit - 1)),
        );

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn init_alloc_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);

        let allocation_size = 1 * PAGE_SIZE as u32;
        let baseline: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code()).message(Request::Allocate(allocation_size)),
        );
        assert_eq!(baseline.result, RunResult::Normal);

        config.alloc_cost = 10000;

        // Allocation cost is withdrawn once for initial page and twice for first allocated page
        // because of how allocator works. Consecutive allocations only charge once per page.
        // Probably will be fixed by https://github.com/gear-tech/gear/issues/12
        let gas_limit = baseline.gas_spent + 3 * config.alloc_cost;

        let mut runner = RunnerContext::with_config(&config);
        let report: RunReport<()> =
            runner.init_program_with_report(InitProgram::from(wasm_code()).message(
                MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit),
            ));

        assert_eq!(report.result, RunResult::Normal);

        let report: RunReport<()> =
            runner.init_program_with_report(InitProgram::from(wasm_code()).message(
                MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit - 1),
            ));

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn init_load_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);

        let baseline: RunReport<()> =
            runner.init_program_with_report(InitProgram::from(wasm_code()).message(Request::Empty));
        assert_eq!(baseline.result, RunResult::Normal);

        config.load_page_cost = 1000;

        // Page load should not occur on program init, because there are no pages no load.
        let gas_limit = baseline.gas_spent + 0 * config.load_page_cost;

        let mut runner = RunnerContext::with_config(&config);
        let report: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code())
                .message(MessageBuilder::from(Request::Empty).gas_limit(gas_limit)),
        );

        assert_eq!(report.result, RunResult::Normal);

        let report: RunReport<()> = runner.init_program_with_report(
            InitProgram::from(wasm_code())
                .message(MessageBuilder::from(Request::Empty).gas_limit(gas_limit - 1)),
        );

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn handle_init_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let allocation_size = PAGE_SIZE as u32;

        // First request with rust code seems to use more gas. Next requests use consistent amounts
        // of gas, so we use them as a baseline, discarding first request result.
        let _warm_up: Reply = runner.request(Request::Allocate(allocation_size));
        let baseline: RunReport<Reply> = runner.request_report(Request::Allocate(allocation_size));

        assert_eq!(baseline.result, RunResult::Normal);

        config.init_cost = 5000;

        // Init cost is not charged because all pages were already initialized. Allocated pages
        // are charged alloc_cost (which is 0 in this test case), not init_cost.
        let gas_limit = baseline.gas_spent + 0 * config.init_cost;

        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let _warm_up: Reply =
            runner.request(MessageBuilder::from(Request::Allocate(allocation_size)));
        let report: RunReport<Reply> = runner.request_report(
            MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit),
        );

        assert_eq!(report.result, RunResult::Normal);

        let report: RunReport<()> = runner.request_report(
            MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit - 1),
        );

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn handle_alloc_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let allocation_size = 2 * PAGE_SIZE as u32;

        // First request with rust code seems to use more gas. Next requests use consistent amounts
        // of gas, so we use them as a baseline, discarding first request result.
        let _warm_up: Reply = runner.request(Request::Allocate(allocation_size));
        let baseline: RunReport<Reply> = runner.request_report(Request::Allocate(allocation_size));

        assert_eq!(baseline.result, RunResult::Normal);

        config.alloc_cost = 10000;

        // Allocation cost is withdrawn twice for first allocated page because of how allocator
        // works. Consecutive allocations only charge once per page. See also [init_alloc_cost]
        // Probably will be fixed by https://github.com/gear-tech/gear/issues/12
        let gas_limit = baseline.gas_spent + 3 * config.alloc_cost;

        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let _warm_up: Reply = runner.request(Request::Allocate(allocation_size));
        let report: RunReport<Reply> = runner.request_report(
            MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit),
        );

        assert_eq!(report.result, RunResult::Normal);

        let report: RunReport<()> = runner.request_report(
            MessageBuilder::from(Request::Allocate(allocation_size)).gas_limit(gas_limit - 1),
        );

        assert_eq!(report.result, RunResult::Trap("Gas limit exceeded".into()));
    }

    #[test]
    fn handle_load_cost() {
        let mut config = Config::default();
        config.alloc_cost = 0;
        config.init_cost = 0;
        config.load_page_cost = 0;
        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let allocation_size = PAGE_SIZE as u32;

        let _set_up: Reply = runner.request(Request::ResizeStatic(allocation_size));
        let _warm_up: Reply = runner.request(Request::Empty);
        let baseline: RunReport<Reply> = runner.request_report(Request::Empty);

        assert_eq!(baseline.result, RunResult::Normal);

        config.load_page_cost = 3000;

        // Load cost is charged for all static pages and 2 extra for first allocated page. See
        // [init_alloc_cost] and [handle_alloc_cost] test cases for details on this behavior
        let gas_limit = baseline.gas_spent + 19 * config.load_page_cost;

        let mut runner = RunnerContext::with_config(&config);
        runner.init_program(wasm_code());

        let _set_up: Reply = runner.request(Request::ResizeStatic(allocation_size));
        let _warm_up: Reply = runner.request(Request::Empty);
        let report1: RunReport<Reply> =
            runner.request_report(MessageBuilder::from(Request::Empty).gas_limit(gas_limit));

        assert_eq!(report1.result, RunResult::Normal);

        let report2: RunReport<Reply> =
            runner.request_report(MessageBuilder::from(Request::Empty).gas_limit(gas_limit - 1));

        assert_eq!(report2.result, RunResult::Trap("Gas limit exceeded".into()));
    }
}
