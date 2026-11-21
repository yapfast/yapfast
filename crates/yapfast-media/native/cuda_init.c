/* The one place libcuda is started, which every shim that dlopens it asks: see
 * cuda_init.h. */
#include "cuda_init.h"

#include <pthread.h>

/* The first caller's call, and what it answered: zero for a driver that is up,
 * and the driver's status otherwise. A later caller reads that answer rather
 * than starting a driver that is running. */
static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static int started = 0;
static int status = -1;

int yapfast_cuda_init(yapfast_cu_init_fn init) {
  pthread_mutex_lock(&lock);
  if (!started) {
    status = init != NULL ? init(0) : -1;
    started = 1;
  }
  pthread_mutex_unlock(&lock);
  return status;
}
