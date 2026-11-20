;; A plugin that never leaves `yf_activate`. The helper's watchdog holds the
;; deadline of every running call and cancels the instance when it passes, so
;; the end-to-end test expects a Fault of kind Deadline and a helper that is
;; still running afterwards. A helper with no deadline hangs on this fixture.
(module
  (func (export "yf_activate")
    (result i32)
    (loop $forever (br $forever))
    (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
