/* libcuda is started once for the process, and `cuInit` is the call that starts
 * it: two threads calling it at once is a race the driver doesn't survive — a
 * decoder's open and an encoder's, which a test binary runs side by side in one
 * process — so every shim that dlopens libcuda asks here, and the first call is
 * the one that makes it. */
#ifndef YAPFAST_CUDA_INIT_H
#define YAPFAST_CUDA_INIT_H

/* The driver's own `cuInit`, named from the library a shim dlopened. `CUresult`
 * is an int in the driver's API, and an enum is passed to it the same way. */
typedef int (*yapfast_cu_init_fn)(unsigned);

/* Starts the driver if nothing has, and answers what it answered then: 0 for a
 * driver that is up, the driver's own status otherwise. */
int yapfast_cuda_init(yapfast_cu_init_fn init);

#endif
