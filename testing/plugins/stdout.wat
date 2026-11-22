;; A plugin that writes to its standard output. The helper's own stdout is the
;; frame stream to Yapfast, so a guest's write reaching it would corrupt every
;; frame after it (PLAN.md 14: "guest stdout can't reach the pipe"). It asks
;; for WASI, the only way a guest has a stdout at all, and on every event calls
;; `fd_write(1, ...)` with "hello\n"; the end-to-end test checks that the next
;; frame still reads. The iovec at 0 points at the six bytes at 16.
(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "\10\00\00\00\06\00\00\00")
  (data (i32.const 16) "hello\0a")
  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32)
    (drop (call $fd_write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8)))
    (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
