#include <cstddef>
#include <functional>

// Prebuilt Android skia-bindings archives target NDK r28+ libc++, which calls
// out-of-line `std::__ndk1::__hash_memory(const void*, size_t)`. When linking
// with NDK r27, provide a weak fallback backed by libc++'s header hash.
#if defined(__ANDROID__) && defined(_LIBCPP_VERSION) && _LIBCPP_VERSION < 190000
namespace std {
inline namespace __ndk1 {
__attribute__((weak)) size_t __hash_memory(const void* ptr, size_t len) {
    return __murmur2_or_cityhash<size_t>()(ptr, len);
}
}  // namespace __ndk1
}  // namespace std
#endif
