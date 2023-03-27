# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.6] 2023-03-16. Complete testing extrinsics set.
### Added
- Whole test chain for `send_reply` and `claim_value` extrinsics.
- Mailbox state in the context, provided to extrinsiscs generator.

## [0.1.5] 2023-01-27. Avoid senders balance exhaustion.
### Added
- Task which renews user's and authority's balance.

## [0.1.4] 2023-01-10. Avoid frequent tx finalization timeout error.
### Added
- Added 2 additional errors that must be handled to avoid frequent finalization wait timeout error on the loader side.

## [0.1.3] 2022-12-21. Make better logging experience.
### Added
- Generating random names of **`adjective`-`noun`** format is introduced. The name is set as a prefix to the current run log file(s). Also the name is logged.
### Changed
- Remove target, file location and line number from stdout logs. They still remain in file logging as it's considered as a more verbose logging.
### Fixed
- Removed double logging to the stdout.
- Removed formatting with ansi symbols for stdout logger.

## [0.1.2] 2022-12-20. Make deterministic generation of seed for code seed generator.
### Added
- Logging seed for code seed generator.
### Changed
- If no code seed type is provided from the CLI, seed for code seed generator will be set as __dynamic__ and will start from a randomly generated value, where random for that value is determined by the loader seed. Previously seed for code seed generator was set to timestamp value by default.

## [0.1.1] 2022-11-21. Increase loader's ux.
### Added
- An ability to run loader starting from the specified seed.
- Logging block hash where queue processing stopped event occurred.
- Send request to `node-stopper` on any `CrashAlert`, not only specific one. Log successful request sending.
- `gear-program` logs are enabled.
- Package name and version are logged.
### Changed
- Crash alerts are captured from lower level error strings by matching them in lowercase format.

## [0.0.1] 2022-11-09. First release.
### Added
- Generator which generates random data, but context-ed data for different `gear` node extrinsics (`send_message`, `upload_code`, `upload_program`, `create_program`).
- Generator for gear program, which works from seed.
- Batch pool which, depending on configurational input params, uses generator to send various extrinsics to `gear` node. Pool also processes results of these extrinsics,
  forming additional context for the generator. While loading the node some suspicious errors can occur, which signal something is wrong on the node (it's down, or panic
  occurred in the runtime). These errors are called `CrashAlert`s and they are reported to the remote process. When `CrashAlert::MsgProcessingStopped` occurs, node is stopped.
- Dump code with specific seed.