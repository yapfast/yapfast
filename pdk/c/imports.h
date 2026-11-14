//! The imports a guest has, and nothing else.
//!
//! Extism supplies the first group to every module it runs, under its own
//! module name; the helper adds the second. A plugin never includes this file:
//! the PDK's own sources do, which is also why these names are short and
//! prefixed with `yf_import_` rather than looking like an API.

#ifndef YAPFAST_IMPORTS_H
#define YAPFAST_IMPORTS_H

#include <stddef.h>
#include <stdint.h>

#define YF_IMPORT(module, name) \
    __attribute__((import_module(module), import_name(name)))

// Extism's kernel. A handle is opaque: it names memory Extism keeps for the
// guest, and only these functions may read or write through one. Casting a
// handle to a C pointer compiles, and reads and writes bytes nobody will ever
// look at, which is why this PDK marshals through store and load instead.
YF_IMPORT("extism:host/env", "input_length") uint64_t yf_import_input_length(void);
YF_IMPORT("extism:host/env", "input_load_u8") uint8_t yf_import_input_load_u8(uint64_t offset);
YF_IMPORT("extism:host/env", "input_load_u64") uint64_t yf_import_input_load_u64(uint64_t offset);
YF_IMPORT("extism:host/env", "alloc") uint64_t yf_import_alloc(uint64_t length);
YF_IMPORT("extism:host/env", "free") void yf_import_free(uint64_t handle);
YF_IMPORT("extism:host/env", "length") uint64_t yf_import_length(uint64_t handle);
YF_IMPORT("extism:host/env", "output_set") void yf_import_output_set(uint64_t handle, uint64_t length);
YF_IMPORT("extism:host/env", "store_u8") void yf_import_store_u8(uint64_t handle, uint8_t value);
YF_IMPORT("extism:host/env", "load_u8") uint8_t yf_import_load_u8(uint64_t offset);
YF_IMPORT("extism:host/env", "store_u64") void yf_import_store_u64(uint64_t handle, uint64_t value);
YF_IMPORT("extism:host/env", "load_u64") uint64_t yf_import_load_u64(uint64_t offset);
YF_IMPORT("extism:host/env", "error_set") void yf_import_error_set(uint64_t handle);

// The helper's own two: one call that is answered, one that isn't. Both take one
// JSON string and, for the first, answer with a handle to JSON.
YF_IMPORT("extism:host/user", "yf_call") uint64_t yf_import_call(uint64_t request);
YF_IMPORT("extism:host/user", "yf_post") void yf_import_post(uint64_t message);

#endif

// The four a freestanding module brings itself. `freestanding.c` defines them
// under these labels, so the compiler's own emitted calls reach the definition
// while the C names stay the PDK's: no plugin source has `memcpy` redefined
// under it.
void *yf_memcpy(void *to, const void *from, size_t n) __asm__("memcpy");
void *yf_memset(void *to, int value, size_t n) __asm__("memset");
void *yf_memmove(void *to, const void *from, size_t n) __asm__("memmove");
int yf_memcmp(const void *a, const void *b, size_t n) __asm__("memcmp");
