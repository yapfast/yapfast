/* libcuda is started once for the process, and `cuInit` is the call that starts
 * it: two threads calling it at once is a race the driver doesn't survive — a
 * decoder's open and an encoder's, which a test binary runs side by side in one
 * process — so every shim that dlopens libcuda asks here, and the first call is
 * the one that makes it. */
#ifndef YAPFAST_CUDA_INIT_H
#define YAPFAST_CUDA_INIT_H

/* `CU_POINTER_ATTRIBUTE_CONTEXT`, the attribute a device pointer is asked for
 * the context it was allocated in, and its number in the driver's API. The
 * vendored dynlink_cuda.h predates the enumeration that names it, and a
 * toolkit's cuda.h is exactly what the shims are built without, so the one
 * value they need is declared here under the prefix native/hwdec_cuda.c gives
 * the other driver names that header doesn't have. */
#define YAPFAST_CU_POINTER_ATTRIBUTE_CONTEXT 1

/* The driver's own `cuInit`, named from the library a shim dlopened. The
 * driver's `CUresult` is an enumeration, which is an int wherever these shims
 * run, so the shim answers in those terms and needs neither a toolkit nor the
 * vendored header to name it: a file that has the enum from dynlink_cuda.h,
 * as native/cudaenc.c and native/cudaconvert.c do, casts the entry point to
 * this type and its answer back, as native/hwdec_cuda.c casts them. */
typedef int (*yapfast_cu_init_fn)(unsigned);

/* Starts the driver if nothing has, and answers what it answered then: 0 for a
 * driver that is up, the driver's own status otherwise. */
int yapfast_cuda_init(yapfast_cu_init_fn init);

#endif
