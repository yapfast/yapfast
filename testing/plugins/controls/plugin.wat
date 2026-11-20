;; Controls: a SpotifyControls-style sample, and one small use of each of
;; the account's own abilities, through the real helper.
;;
;;   voice.panel     Back, Pause or Play, and Next while in a call:
;;                   `spotify.previous`, `spotify.pause`/`spotify.play`,
;;                   `spotify.next`, with the label following `spotify_state`
;;   keybinds        `back`, `toggle` and `next`, which Settings → Keybinds
;;                   offers as "Controls: …"; the plugin hears the action only
;;   composer        Share logo: `messages.attach` with the package's own
;;                   `logo` asset, in the conversation the button is under
;;   sounds          the package's `beep` in place of Yapfast's message sound
;;   /controls-activity  `activity.set`: "Listening to Controls sample"
;;   /controls-clear     `activity.clear`
;;   /controls-now       `spotify.now`, answered as the command's value
;;
;; The input is told apart by the first letter after `"id":"` or
;; `"action":"`: b(ack), t(oggle), n(ext) and s(hare). Every literal lives in
;; this module's memory and is copied into the kernel's one byte at a time,
;; as `showcase/plugin.wat` does.
(module
  (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
  (import "extism:host/env" "length" (func $length (param i64) (result i64)))
  (import "extism:host/env" "load_u8" (func $load_u8 (param i64) (result i32)))
  (import "extism:host/env" "store_u8" (func $store_u8 (param i64 i32)))
  (import "extism:host/env" "input_length" (func $input_length (result i64)))
  (import "extism:host/env" "input_load_u8" (func $input_load_u8 (param i64) (result i32)))
  (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
  (import "extism:host/user" "yf_call" (func $yf_call (param i64) (result i64)))

  (memory 1)
  ;; panel_pause: 222 bytes
  (data (i32.const 0) "{\"method\":\"ui.set_slot\",\"payload\":{\"slot\":\"voice.panel\",\"tree\":{\"rows\":[{\"cells\":[{\"kind\":\"button\",\"id\":\"back\",\"text\":\"Back\"},{\"kind\":\"button\",\"id\":\"toggle\",\"text\":\"Pause\"},{\"kind\":\"button\",\"id\":\"next\",\"text\":\"Next\"}]}]}}}")
  ;; panel_play: 221 bytes
  (data (i32.const 224) "{\"method\":\"ui.set_slot\",\"payload\":{\"slot\":\"voice.panel\",\"tree\":{\"rows\":[{\"cells\":[{\"kind\":\"button\",\"id\":\"back\",\"text\":\"Back\"},{\"kind\":\"button\",\"id\":\"toggle\",\"text\":\"Play\"},{\"kind\":\"button\",\"id\":\"next\",\"text\":\"Next\"}]}]}}}")
  ;; composer: 135 bytes
  (data (i32.const 448) "{\"method\":\"ui.set_slot\",\"payload\":{\"slot\":\"composer\",\"tree\":{\"rows\":[{\"cells\":[{\"kind\":\"button\",\"id\":\"share\",\"text\":\"Share logo\"}]}]}}}")
  ;; previous: 29 bytes
  (data (i32.const 584) "{\"method\":\"spotify.previous\"}")
  ;; next: 25 bytes
  (data (i32.const 616) "{\"method\":\"spotify.next\"}")
  ;; pause: 26 bytes
  (data (i32.const 648) "{\"method\":\"spotify.pause\"}")
  ;; play: 25 bytes
  (data (i32.const 680) "{\"method\":\"spotify.play\"}")
  ;; now: 24 bytes
  (data (i32.const 712) "{\"method\":\"spotify.now\"}")
  ;; activity: 196 bytes
  (data (i32.const 744) "{\"method\":\"activity.set\",\"payload\":{\"kind\":\"listening\",\"name\":\"Controls sample\",\"details\":\"Skipping songs from Yapfast\",\"buttons\":[{\"label\":\"Yapfast\",\"url\":\"https://github.com/yapfast/yapfast\"}]}}")
  ;; clear: 27 bytes
  (data (i32.const 944) "{\"method\":\"activity.clear\"}")
  ;; attach_head: 47 bytes
  (data (i32.const 976) "{\"method\":\"messages.attach\",\"payload\":{\"room\":\"")
  ;; attach_tail: 93 bytes
  (data (i32.const 1024) "\",\"body\":\"Sent with the Controls sample\",\"attachments\":[{\"name\":\"logo.png\",\"asset\":\"logo\"}]}}")
  ;; value: 9 bytes
  (data (i32.const 1120) "{\"value\":")
  ;; pat_id: 6 bytes
  (data (i32.const 1136) "\"id\":\"")
  ;; pat_action: 10 bytes
  (data (i32.const 1144) "\"action\":\"")
  ;; pat_subject: 11 bytes
  (data (i32.const 1160) "\"subject\":\"")
  ;; pat_playing_true: 14 bytes
  (data (i32.const 1176) "\"playing\":true")
  ;; pat_playing_false: 15 bytes
  (data (i32.const 1192) "\"playing\":false")

  ;; Whether Spotify said it plays, from the last `spotify_state`.
  (global $playing (mut i32) (i32.const 1))

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

  ;; Calls the host with the literal at `from`, and answers what it said.
  (func $ask (param $from i32) (param $length i32) (result i64)
    (call $yf_call (call $copy_out (local.get $from) (i64.extend_i32_u (local.get $length)))))

  ;; Where the input has the literal at `pattern`, the offset just past it,
  ;; or -1.
  (func $find (param $pattern i32) (param $length i32) (result i64)
    (local $at i64)
    (local $i i32)
    (local $end i64)
    (local.set $end (i64.sub (call $input_length) (i64.extend_i32_u (local.get $length))))
    (block $none
      (loop $next
        (br_if $none (i64.gt_s (local.get $at) (local.get $end)))
        (local.set $i (i32.const 0))
        (block $differs
          (loop $compare
            (if (i32.ge_u (local.get $i) (local.get $length))
              (then (return (i64.add (local.get $at) (i64.extend_i32_u (local.get $length))))))
            (br_if $differs
              (i32.ne
                (call $input_load_u8 (i64.add (local.get $at) (i64.extend_i32_u (local.get $i))))
                (i32.load8_u (i32.add (local.get $pattern) (local.get $i)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $compare)))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $next)))
    (i64.const -1))

  ;; The voice panel's buttons, with Pause or Play as Spotify last said.
  (func $panel
    (if (global.get $playing)
      (then (drop (call $ask (i32.const 0) (i32.const 222))))
      (else (drop (call $ask (i32.const 224) (i32.const 221))))))

  ;; Sends the package's logo in the conversation whose id starts at `at`
  ;; in the input and runs to the next quote, as `messages.attach`.
  (func $share (param $at i64)
    (local $out i32)
    (local $byte i32)
    (memory.copy (i32.const 4096) (i32.const 976) (i32.const 47))
    (local.set $out (i32.const 4143))
    (block $done
      (loop $copy
        (local.set $byte (call $input_load_u8 (local.get $at)))
        (br_if $done (i32.eq (local.get $byte) (i32.const 34)))
        (br_if $done (i32.ge_u (local.get $out) (i32.const 8000)))
        (i32.store8 (local.get $out) (local.get $byte))
        (local.set $out (i32.add (local.get $out) (i32.const 1)))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $copy)))
    (memory.copy (local.get $out) (i32.const 1024) (i32.const 93))
    (drop (call $ask (i32.const 4096)
      (i32.sub (i32.add (local.get $out) (i32.const 93)) (i32.const 4096)))))

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
          (i32.load8_u (i32.add (i32.const 1120) (i32.wrap_i64 (local.get $at)))))
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

  (func (export "yf_activate") (result i32)
    (call $panel)
    (drop (call $ask (i32.const 448) (i32.const 135)))
    (i32.const 0))
  (func (export "yf_deactivate") (result i32) (i32.const 0))
  ;; `spotify_state` turns the middle button into Pause or Play.
  (func (export "yf_event") (result i32)
    (if (i64.ge_s (call $find (i32.const 1176) (i32.const 14)) (i64.const 0))
      (then (global.set $playing (i32.const 1)) (call $panel))
      (else
        (if (i64.ge_s (call $find (i32.const 1192) (i32.const 15)) (i64.const 0))
          (then (global.set $playing (i32.const 0)) (call $panel)))))
    (i32.const 0))
  (func (export "yf_hook") (result i32) (i32.const 0))
  (func (export "yf_decorate") (result i32) (i32.const 0))
  ;; A button in the voice panel or the composer, or a keybind.
  (func (export "yf_input") (result i32)
    (local $at i64)
    (local $which i32)
    (local.set $at (call $find (i32.const 1136) (i32.const 6)))
    (if (i64.lt_s (local.get $at) (i64.const 0))
      (then (local.set $at (call $find (i32.const 1144) (i32.const 10)))))
    (if (i64.lt_s (local.get $at) (i64.const 0)) (then (return (i32.const 0))))
    (local.set $which (call $input_load_u8 (local.get $at)))
    (if (i32.eq (local.get $which) (i32.const 98)) ;; b
      (then (drop (call $ask (i32.const 584) (i32.const 29))) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 110)) ;; n
      (then (drop (call $ask (i32.const 616) (i32.const 25))) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 116)) ;; t
      (then
        (if (global.get $playing)
          (then (drop (call $ask (i32.const 648) (i32.const 26))) (global.set $playing (i32.const 0)))
          (else (drop (call $ask (i32.const 680) (i32.const 25))) (global.set $playing (i32.const 1))))
        (call $panel)
        (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 115)) ;; s
      (then
        (local.set $at (call $find (i32.const 1160) (i32.const 11)))
        (if (i64.ge_s (local.get $at) (i64.const 0)) (then (call $share (local.get $at))))))
    (i32.const 0))
  (func (export "yf_command") (result i32)
    (local $which i32)
    (local.set $which (call $input_load_u8 (i64.const 18)))
    (if (i32.eq (local.get $which) (i32.const 97)) ;; a
      (then (call $answer_value (call $ask (i32.const 744) (i32.const 196))) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 99)) ;; c
      (then (call $answer_value (call $ask (i32.const 944) (i32.const 27))) (return (i32.const 0))))
    (if (i32.eq (local.get $which) (i32.const 110)) ;; n
      (then (call $answer_value (call $ask (i32.const 712) (i32.const 24))) (return (i32.const 0))))
    (i32.const 0))
)
