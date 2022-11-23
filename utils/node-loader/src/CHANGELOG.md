# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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