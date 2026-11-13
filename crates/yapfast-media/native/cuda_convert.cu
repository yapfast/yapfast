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
 * filtering scaler samples: a destination pixel reads the source at its centre.
 *
 * A screen showing HDR hands its frames over as HDR's codes — PQ's, with the
 * wide gamut they are mastered in — and `bgra_frame.tone` says so. Then the
 * sample is read as the light it stands for, shrunk into BT.709's gamut and
 * written as the code an SDR screen shows that light as, before the matrix
 * below sums it: the stream is SDR's whatever the screen shows, and there is no
 * second pass over the picture for it. A screen showing SDR leaves `tone` zero,
 * and every number in this file is the one it was. */

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
  /* Red in the first byte rather than blue: the array behind a GL texture,
   * which holds the picture as GL does. */
  int red_first;
  /* These are HDR's codes, and a sample is curved into SDR's on its way out:
   * a screen showing HDR writes its frames this way, and a share of one is its
   * picture rather than its codes read as SDR. */
  int tone;
  /* A texture object over that array, read texel by texel where it isn't
   * zero: the picture where the driver tiled it, rather than a copy of its
   * rows. `pixels` and `pitch` are unused then. */
  unsigned long long texture;
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

/* The base two logarithm of a positive number: the exponent its bits carry,
 * and a series over the mantissa, which lies between one and two and so leaves
 * the series' own scale — `s` above a third — small enough to stop soon. This
 * is written out because a kernel here has no maths library to ask: `powf` is a
 * call into one this build doesn't link (`build.rs` compiles PTX with no
 * toolkit), and the two powers the curve below needs are the one this gives and
 * the hardware's own power of two. */
__device__ static float log2_of(float value) {
  unsigned bits = __builtin_bit_cast(unsigned, value);
  int exponent = (int)((bits >> 23) & 0xffu) - 127;
  /* The mantissa alone, as a float of its own: the same bits without the
   * exponent, which is a number from one up to two. */
  float mantissa = __builtin_bit_cast(float, (bits & 0x007fffffu) | 0x3f800000u);
  float s = (mantissa - 1.0f) / (mantissa + 1.0f);
  float s2 = s * s;
  /* log of the mantissa, which is twice its atanh: the terms to s to the
   * seventh are within a hundred-thousandth of it, far inside what an 8-bit
   * sample can tell. */
  float series = s * (2.0f + s2 * (2.0f / 3.0f + s2 * (2.0f / 5.0f + s2 * (2.0f / 7.0f))));
  return (float)exponent + series * 1.4426950408889634f;
}

/* `value` raised to `inverse`: a logarithm, a multiplication and the power of
 * two the card has an instruction for. A value of zero has no logarithm and
 * doesn't need one — the answer is its own. */
__device__ static float raised(float value, float inverse) {
  if (value <= 0.0f) return 0.0f;
  return __builtin_exp2f(log2_of(value) * inverse);
}

/* The light a PQ code stands for, in nits: SMPTE ST 2084's inverse EOTF, which
 * is what a screen showing HDR writes its frames with, and the curve
 * `native/picture.c` reads the same codes through on the processor. */
__device__ static float light_of(float code) {
  const float m1 = 2610.0f / 16384.0f;
  const float m2 = 2523.0f / 4096.0f * 128.0f;
  const float c1 = 3424.0f / 4096.0f;
  const float c2 = 2413.0f / 4096.0f * 32.0f;
  const float c3 = 2392.0f / 4096.0f * 32.0f;
  float raised_code = raised(code, 1.0f / m2);
  float base = (raised_code - c1) / (c2 - c3 * raised_code);
  return raised(base, 1.0f / m1) * 10000.0f;
}

/* The code an SDR screen shows `nits` of light as, 0 to 255: the straight line
 * up to SDR white kept as it is, the room above it rolled off through a
 * Reinhard shoulder — so brighter stays brighter and highlights keep their
 * order instead of flattening into one another — and sRGB's curve, which is how
 * the matrix below reads the sample. The numbers are `picture::Tone::pq()`'s,
 * so the card's picture and the processor's are the same picture. */
__device__ static float code_of(float nits) {
  const float white = 203.0f;  /* SDR white in nits, BT.2408's reference */
  const float knee = 0.75f;    /* where the straight line stops */
  const float peak = 4000.0f;  /* the brightest content is mastered to */
  float relative = nits / white;
  float shown = relative;
  if (relative > knee) {
    float above = clamp((relative - knee) / (peak / white - knee), 0.0f, 1.0f);
    shown = knee + (1.0f - knee) * 2.0f * above / (1.0f + above);
  }
  shown = clamp(shown, 0.0f, 1.0f);
  float code = shown <= 0.0031308f ? 12.92f * shown
                                  : 1.055f * raised(shown, 1.0f / 2.4f) - 0.055f;
  return code * 255.0f;
}

/* A sample of an HDR picture as the SDR one it is: each channel read as the
 * light it stands for, the wide gamut shrunk into BT.709's, and the code a
 * screen showing SDR shows that light as. A colour BT.709 has no way to show
 * comes back negative from the matrix and the curve takes it to none of that
 * channel, which is the clamp; a neutral sample has equal light in all three
 * and the matrix leaves it exactly as it was. */
__device__ static struct vec4 sdr_of(struct vec4 pixel) {
  /* The order the kernels keep: x is blue, y green and z red. */
  float red = light_of(pixel.z / 255.0f);
  float green = light_of(pixel.y / 255.0f);
  float blue = light_of(pixel.x / 255.0f);
  /* BT.2087's matrix, in the light the codes stand for. */
  float to_red = 1.6605f * red - 0.5876f * green - 0.0729f * blue;
  float to_green = -0.1246f * red + 1.1329f * green - 0.0083f * blue;
  float to_blue = -0.0182f * red - 0.1006f * green + 1.1187f * blue;
  struct vec4 out;
  out.x = code_of(to_blue);
  out.y = code_of(to_green);
  out.z = code_of(to_red);
  out.w = 1.0f;
  return out;
}

/* The first three channels of the pixel at `x`, `y`, in the order they lie. */
__device__ static struct vec4 texel(struct bgra_frame frame, int x, int y) {
  struct vec4 out;
  if (frame.texture != 0) {
    /* An integer read of the array at whole texel coordinates: each channel as
     * the byte it is, with no filtering, as the rows below are read. */
    uint r, g, b, a;
    asm volatile("tex.2d.v4.u32.s32 {%0, %1, %2, %3}, [%4, {%5, %6}];"
                 : "=r"(r), "=r"(g), "=r"(b), "=r"(a)
                 : "l"(frame.texture), "r"(x), "r"(y));
    out.x = (float)r;
    out.y = (float)g;
    out.z = (float)b;
  } else {
    const uchar *p = frame.pixels + (unsigned long)y * frame.pitch + (unsigned long)x * 4;
    out.x = p[0];
    out.y = p[1];
    out.z = p[2];
  }
  out.w = 1.0f;
  return out;
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
  struct vec4 a = texel(frame, left, top);
  struct vec4 b = texel(frame, right, top);
  struct vec4 c = texel(frame, left, bottom);
  struct vec4 d = texel(frame, right, bottom);
  struct vec4 out;
  /* DRM's BGRA8888 is blue first in memory; the stream's matrix wants red,
   * green and blue. */
  out.x = (a.x * (1 - fx) + b.x * fx) * (1 - fy) + (c.x * (1 - fx) + d.x * fx) * fy;
  out.y = (a.y * (1 - fx) + b.y * fx) * (1 - fy) + (c.y * (1 - fx) + d.y * fx) * fy;
  out.z = (a.z * (1 - fx) + b.z * fx) * (1 - fy) + (c.z * (1 - fx) + d.z * fx) * fy;
  out.w = 1.0f;
  if (frame.red_first) {
    float red = out.x;
    out.x = out.z;
    out.z = red;
  }
  if (frame.tone != 0) out = sdr_of(out);
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
