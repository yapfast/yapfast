;; The Showcase: one small plugin that uses each ability added with the
;; `voice.read`, `files.save`, `audio.play` and asset work, and the render hook,
;; so a person or a test can see each one through the real helper.
;;
;;   /showcase-play   `audio.play` with the package's own `beep` asset
;;   /showcase-file   `files.save`: the save dialog, titled with the plugin
;;   /showcase-call   `voice.state`, answered as the command's value
;;   /showcase-list   `assets.list`, the same way
;;   yf_decorate      an annotation, two marks and a card under every message
;;
;; A command's answer is `{"value": <the host's answer>}`, which the parent
;; logs, so the answer to each call can be read without a PDK. The command is
;; told apart by the 19th byte of its JSON, `{"name":"showcase-?`, which is
;; `p`, `f`, `c` or `l`. The literals live in this module's memory and are
;; copied into the kernel's, one byte at a time, as `slow_host.wat` does.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "length" (func $length (param i64) (result i64)))
  (import "extism:host/env" "load_u8" (func $load_u8 (param i64) (result i32)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
  (import "extism:host/user" "yf_call" (func $yf_call (param i64) (result i64)))

  (memory 1)
  ;; play: 62 bytes
  (data (i32.const 0) "{\"method\":\"audio.play\",\"payload\":{\"asset\":\"beep\",\"volume\":60}}")
  ;; file: 140 bytes
  (data (i32.const 70) "{\"method\":\"files.save\",\"payload\":{\"name\":\"showcase.txt\",\"text\":\"Saved by the Showcase plugin. It chose these words; you chose the folder.\"}}")
  ;; call: 24 bytes
  (data (i32.const 218) "{\"method\":\"voice.state\"}")
  ;; list: 24 bytes
  (data (i32.const 250) "{\"method\":\"assets.list\"}")
  ;; decor: 333 bytes
  (data (i32.const 282) "{\"annotation\":{\"text\":\"a line under the message, from the plugin and never part of it\",\"tone\":\"accent\"},\"marks\":[{\"text\":\"preview\",\"style\":\"highlight\"},{\"text\":\"game\",\"style\":\"bold\"}],\"tree\":{\"rows\":[{\"cells\":[{\"kind\":\"badge\",\"text\":\"Showcase\",\"tone\":\"accent\"},{\"kind\":\"text\",\"text\":\"A card the plugin drew under this message.\"}]}]}}")
  ;; prefix: 9 bytes
  (data (i32.const 626) "{\"value\":")

  ;; Copies `length` bytes of this module's memory from `from` into a fresh
  ;; allocation in the kernel's, and answers its offset.
  (func $copy_out (param $from i32) (param $length i64) (result i64)
    (local $buffer i64)
    (local $at i64)
    (local.set $buffer (call $alloc (local.get $length)))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (local.get $length)))
        (call $store_u8
          (i64.add (local.get $buffer) (local.get $at))
          (i32.load8_u (i32.add (local.get $from) (i32.wrap_i64 (local.get $at)))))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (local.get $buffer))

  ;; Makes the command's answer `{"value":` + the host's answer + `}`.
  (func $answer_value (param $answer i64)
    (local $length i64)
    (local $buffer i64)
    (local $at i64)
    (local.set $length (call $length (local.get $answer)))
    (local.set $buffer (call $alloc (i64.add (local.get $length) (i64.const 10))))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (i64.const 9)))
        (call $store_u8
          (i64.add (local.get $buffer) (local.get $at))
          (i32.load8_u (i32.add (i32.const 626) (i32.wrap_i64 (local.get $at)))))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (local.set $at (i64.const 0))
    (block $done
      (loop $copy
        (br_if $done (i64.ge_u (local.get $at) (local.get $length)))
        (call $store_u8
          (i64.add (local.get $buffer) (i64.add (i64.const 9) (local.get $at)))
          (call $load_u8 (i64.add (local.get $answer) (local.get $at))))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (call $store_u8
      (i64.add (local.get $buffer) (i64.add (i64.const 9) (local.get $length)))
      (i32.const 125))
    (call $output_set
      (local.get $buffer)
      (i64.add (local.get $length) (i64.const 10))))

  ;; Calls the host with the literal at `from`, and answers with what it said.
  (func $ask (param $from i32) (param $length i64)
    (call $answer_value
      (call $yf_call (call $copy_out (local.get $from) (local.get $length)))))

  (func (export "yf_activate") (result i32) (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  ;; Answers to `pending` calls arrive here; the parent logs nothing for them,
  ;; and the plugin needs nothing from them.
  (func (export "yf_event") (result i32) (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_command")
    (result i32)
    (local $which i32)
    (local.set $which (call $input_load_u8 (i64.const 18)))
    (if (i32.eq (local.get $which) (i32.const 112)) ;; p
      (then (call $ask (i32.const 0) (i64.const 62)) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 102)) ;; f
      (then (call $ask (i32.const 70) (i64.const 140)) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 99)) ;; c
      (then (call $ask (i32.const 218) (i64.const 24)) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 108)) ;; l
      (then (call $ask (i32.const 250) (i64.const 24)) (return (i32.const 0))))
    (i32.const 0))
  ;; The same decoration under every message: a line, two marks and a card.
  (func (export "yf_decorate")
    (result i32)
    (call $output_set
      (call $copy_out (i32.const 282) (i64.const 333))
      (i64.const 333))
    (i32.const 0))
)
