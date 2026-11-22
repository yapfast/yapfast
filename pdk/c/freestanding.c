//! What a freestanding module has to bring for itself.
//!
//! A plugin is compiled with no C library, because there is none to link: it runs
//! in Extism, which gives a guest memory and imports and nothing else. Clang
//! still emits calls to these four for struct copies, for clearing a buffer and
//! for comparisons, so the module has to define them or fail to link.
//!
//! Each is written under a name of the PDK's own with an assembler label that
//! says what the compiler asked for. Naming them `memcpy` and the rest would
//! shadow a standard name in every plugin's source; this way the symbol the
//! linker wants exists and the C identifier stays the PDK's.

#include <stddef.h>

void *yf_memcpy(void *to, const void *from, size_t n) __asm__("memcpy");
void *yf_memset(void *to, int value, size_t n) __asm__("memset");
void *yf_memmove(void *to, const void *from, size_t n) __asm__("memmove");
int yf_memcmp(const void *a, const void *b, size_t n) __asm__("memcmp");

void *yf_memcpy(void *to, const void *from, size_t n) {
    unsigned char *out = to;
    const unsigned char *in = from;
    for (size_t i = 0; i < n; i++) out[i] = in[i];
    return to;
}

void *yf_memset(void *to, int value, size_t n) {
    unsigned char *out = to;
    for (size_t i = 0; i < n; i++) out[i] = (unsigned char)value;
    return to;
}

void *yf_memmove(void *to, const void *from, size_t n) {
    unsigned char *out = to;
    const unsigned char *in = from;
    if (out < in) {
        for (size_t i = 0; i < n; i++) out[i] = in[i];
    } else {
        for (size_t i = n; i > 0; i--) out[i - 1] = in[i - 1];
    }
    return to;
}

int yf_memcmp(const void *a, const void *b, size_t n) {
    const unsigned char *one = a;
    const unsigned char *two = b;
    for (size_t i = 0; i < n; i++) {
        if (one[i] != two[i]) return one[i] < two[i] ? -1 : 1;
    }
    return 0;
}
