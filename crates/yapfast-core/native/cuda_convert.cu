/* The convert and scale a screen's DMA-BUF goes through on NVIDIA's card:
 * BGRA or BGRX pixels (or RGBA, from GL's array) come out as the NV12 a NVENC session reads, at the
 * stream's size rather than the screen's.
 *
 * This is the only CUDA C in the build, and it is compiled to PTX at build time
 * with clang (see `build.rs`), so a machine that runs Yapfast needs NVIDIA's
 * driver and nothing else: no CUDA toolkit, no `nvcc`, no NPP, and no CUDA
 * headers either — the few types and builtins used here are declared below, and
 * clang's own `__clang_cuda_builtin_vars.h` gives the launch grid. The PTX is
 * loaded with `cuModuleLoadData` in `native/cudaconvert.c`, which is also where
 * the import and the launch live.
 *
 * Color is BT.601 limited range, the matrix and range a stream declares and the
 * one libyuv makes of the same picture on the processor (`hwenc.c` hands NVENC
 * BT.601 too), so a share's picture doesn't change tone when it moves between
 * the card's path and the processor's. Scaling is bilinear on the grid libyuv's
 * filtering scaler samples: a destination pixel reads the source at its centre. */

typedef unsigned char uchar;
typedef unsigned int uint;

struct vec4 {
  float x, y, z, w;
};

struct vec2 {
  float x, y;
};

struct bgra_frame {
  const uchar *pixels;
  uint pitch;
  int width, height;
  /* Red in the first byte rather than blue: the rows copied out of the array
   * behind a GL texture, which holds the picture as GL does. */
  int red_first;
};

struct nv12_frame {
  uchar *pixels;
  uint pitch;
  int width, height;
};

__device__ static float clamp(float value, float low, float high) {
  return value < low ? low : (value > high ? high : value);
}

/* Rounds to the nearest whole 8-bit sample, keeping it in range. */
__device__ static uchar byte_of(float value) {
  return (uchar)clamp(value + 0.5f, 0.0f, 255.0f);
}

/* The four channels at one point of the source, bilinearly interpolated. */
__device__ static struct vec4 source_at(struct bgra_frame frame, float x, float y) {
  x = clamp(x, 0.0f, (float)(frame.width - 1));
  y = clamp(y, 0.0f, (float)(frame.height - 1));
  int left = (int)x;
  int top = (int)y;
  int right = left + 1 < frame.width ? left + 1 : left;
  int bottom = top + 1 < frame.height ? top + 1 : top;
  float fx = x - (float)left;
  float fy = y - (float)top;
  const uchar *a = frame.pixels + (unsigned long)top * frame.pitch + (unsigned long)left * 4;
  const uchar *b = frame.pixels + (unsigned long)top * frame.pitch + (unsigned long)right * 4;
  const uchar *c = frame.pixels + (unsigned long)bottom * frame.pitch + (unsigned long)left * 4;
  const uchar *d = frame.pixels + (unsigned long)bottom * frame.pitch + (unsigned long)right * 4;
  struct vec4 out;
  /* DRM's BGRA8888 is blue first in memory; the stream's matrix wants red,
   * green and blue. */
  out.x = (a[0] * (1 - fx) + b[0] * fx) * (1 - fy) + (c[0] * (1 - fx) + d[0] * fx) * fy;
  out.y = (a[1] * (1 - fx) + b[1] * fx) * (1 - fy) + (c[1] * (1 - fx) + d[1] * fx) * fy;
  out.z = (a[2] * (1 - fx) + b[2] * fx) * (1 - fy) + (c[2] * (1 - fx) + d[2] * fx) * fy;
  out.w = 1.0f;
  if (frame.red_first) {
    float red = out.x;
    out.x = out.z;
    out.z = red;
  }
  return out;
}

/* The place a destination pixel's centre falls on in the source. */
__device__ static struct vec2 source_point(struct bgra_frame frame, struct nv12_frame nv12,
                                          int x, int y) {
  struct vec2 at;
  at.x = ((float)x + 0.5f) * (float)frame.width / (float)nv12.width - 0.5f;
  at.y = ((float)y + 0.5f) * (float)frame.height / (float)nv12.height - 0.5f;
  return at;
}

/* One luma sample for each pixel of the stream's picture. */
extern "C" __global__ void bgra_to_y(struct bgra_frame frame, struct nv12_frame nv12) {
  int x = blockIdx.x * blockDim.x + threadIdx.x;
  int y = blockIdx.y * blockDim.y + threadIdx.y;
  if (x >= nv12.width || y >= nv12.height) return;
  struct vec2 at = source_point(frame, nv12, x, y);
  struct vec4 pixel = source_at(frame, at.x, at.y);
  nv12.pixels[(unsigned long)y * nv12.pitch + x] =
      byte_of(16.0f + 0.256788f * pixel.z + 0.504129f * pixel.y + 0.097906f * pixel.x);
}

/* One pair of chroma samples for each two by two block, averaged from the four
 * places the block's own luma samples come from: what a 4:2:0 picture keeps of
 * its colour. */
extern "C" __global__ void bgra_to_uv(struct bgra_frame frame, struct nv12_frame nv12) {
  int block_x = blockIdx.x * blockDim.x + threadIdx.x;
  int block_y = blockIdx.y * blockDim.y + threadIdx.y;
  int x = block_x * 2;
  int y = block_y * 2;
  if (x >= nv12.width || y >= nv12.height) return;
  struct vec2 centre = source_point(frame, nv12, x, y);
  float step_x = (float)frame.width / (float)nv12.width;
  float step_y = (float)frame.height / (float)nv12.height;
  struct vec4 sum = source_at(frame, centre.x, centre.y);
  struct vec4 across = source_at(frame, centre.x + step_x, centre.y);
  struct vec4 below = source_at(frame, centre.x, centre.y + step_y);
  struct vec4 corner = source_at(frame, centre.x + step_x, centre.y + step_y);
  float blue = (sum.x + across.x + below.x + corner.x) * 0.25f;
  float green = (sum.y + across.y + below.y + corner.y) * 0.25f;
  float red = (sum.z + across.z + below.z + corner.z) * 0.25f;
  /* BT.601 limited range, 4:2:0 interleaved as NV12 keeps it: U then V beside
   * each other, a row of them for each two rows of luma. */
  float u = 128.0f - 0.148223f * red - 0.290993f * green + 0.439216f * blue;
  float v = 128.0f + 0.439216f * red - 0.367788f * green - 0.071427f * blue;
  uchar *row = nv12.pixels + (unsigned long)nv12.pitch * (unsigned long)nv12.height +
               (unsigned long)(y / 2) * nv12.pitch + (unsigned long)x;
  row[0] = byte_of(u);
  row[1] = byte_of(v);
}
