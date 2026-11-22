;; A plugin that traps on its first call: `unreachable` in `yf_activate`. The
;; runtime reports it as a Fault of kind Trap, discards the instance and strikes
;; the plugin; the end-to-end test also checks that the helper keeps serving
;; other plugins afterwards.
(module
  (func (export "yf_activate") (result i32) (unreachable))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
)
