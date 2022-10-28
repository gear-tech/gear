(module
  (type $t0 (func (param i32) (result i32)))
  (type $t1 (func (param i32)))
  (type $t2 (func))
  (type $t3 (func (param i32 i32)))
  (type $t4 (func (result i64)))
  (type $t5 (func (param i32 i32 i32 i32 i32 i32) (result i32)))
  (type $t6 (func (param i32 i32 i32 i32 i32) (result i32)))
  (type $t7 (func (param i32 i32 i64 i32 i32 i32) (result i32)))
  (type $t8 (func (param i32 i32 i32 i64 i32 i32 i32) (result i32)))
  (import "env" "gr_debug" (func $env.gr_debug (type $t3)))
  (import "env" "gr_error" (func $env.gr_error (type $t0)))
  (import "env" "gr_status_code" (func $env.gr_status_code (type $t0)))
  (import "env" "gr_gas_available" (func $env.gr_gas_available (type $t4)))
  (import "env" "gr_reply_to" (func $env.gr_reply_to (type $t0)))
  (import "env" "gr_send" (func $env.gr_send (type $t5)))
  (import "env" "gr_send_commit" (func $env.gr_send_commit (type $t6)))
  (import "env" "gr_send_commit_wgas" (func $env.gr_send_commit_wgas (type $t7)))
  (import "env" "gr_send_init" (func $env.gr_send_init (type $t0)))
  (import "env" "gr_send_wgas" (func $env.gr_send_wgas (type $t8)))
  (import "env" "gr_value_available" (func $env.gr_value_available (type $t1)))
  (import "env" "gr_wait_up_to" (func $env.gr_wait_up_to (type $t1)))
  (import "env" "memory" (memory $env.memory 417))
  (func $init (export "init") (type $t2)
    (call $env.gr_debug
      (i32.const 0)
      (i32.const 41))
    (call $env.gr_value_available
      (i32.const 26620749))
    (drop
      (call $env.gr_send_wgas
        (i32.const 22026608)
        (i32.const 17033664)
        (i32.const 63313)
        (i64.const 109194200382)
        (i32.const 4805440)
        (i32.const 5798)
        (i32.const 8481826)))
    (drop
      (call $env.gr_gas_available)))
  (func $handle (export "handle") (type $t2)
    (drop
      (call $env.gr_status_code
        (i32.const 31356100)))
    (drop
      (call $env.gr_send_init
        (i32.const 12)))
    (drop
      (call $env.gr_send_commit
        (i32.const 75)
        (i32.const 26062759)
        (i32.const 12242275)
        (i32.const 4297)
        (i32.const 12061345)))
    (drop
      (call $env.gr_reply_to
        (i32.const 14563854)))
    (drop
      (call $env.gr_error
        (i32.const 14671505)))
    (drop
      (call $env.gr_send_commit_wgas
        (i32.const 35)
        (i32.const 4904192)
        (i64.const 164933298933)
        (i32.const 18299979)
        (i32.const 5653)
        (i32.const 20410223)))
    (drop
      (call $env.gr_send
        (i32.const 14860294)
        (i32.const 29195482)
        (i32.const 42804)
        (i32.const 25147948)
        (i32.const 2296)
        (i32.const 25886328)))
    (call $env.gr_wait_up_to
      (i32.const 46583)))
  (data $d0 (i32.const 0) "Gear program seed = '1638883285089470236'"))
