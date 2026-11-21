/* The tone map an HDR screen's picture goes through on the card, in HLSL, for the
 * two shims that need it: the one that scales a captured picture into the picture
 * an NVENC session reads (native/d3d11enc.c) and the one that tone-maps it into
 * the BGRA the video processor turns into NV12 for an encoder that reads surfaces
 * — Quick Sync's and AMF's (native/d3d11convert.c).
 *
 * It is one curve in one place because it is one answer: what a screen showing
 * HDR draws, written as the picture an SDR screen shows. Windows Graphics Capture
 * hands the HDR desktop over as scRGB — linear light, BT.709's primaries, 1.0 at
 * 80 nits — and 80 nits is not white. Windows composites plain SDR content at the
 * brightness its display settings call SDR white, 240 nits on a fresh screen, so
 * an SDR white arrives at 3.0 in that surface: dividing by 1.0 sends every window
 * out three times too bright with everything above a third of white clipped, and
 * dividing by the screen's own answer puts a grey at the grey the screen shows.
 * The light above nine tenths of that is a highlight the screen can show and an
 * 8-bit frame cannot, so it rolls off short of white instead of clipping: a
 * 1000-nit specular arrives as white rather than as a flat patch with a hard edge.
 *
 * `white` is that divisor, in scRGB's units — what the system reports as
 * DISPLAYCONFIG_SDR_WHITE_LEVEL, in thousandths of 80 nits, over 1000 — and 1.0
 * is the reading a caller that wasn't told anything gets. A caller's constant
 * reaches the shader however it likes; this only names the argument.
 *
 * `yapfast_srgb_encode` is the sRGB transfer function, piecewise as it is
 * defined: a 2.2 power is close and is wrong by up to two code values in the
 * shadows, which is the sort of wrong a viewer comparing a share against their
 * own screen sees. `yapfast_tone` is the whole map: the light divided by what the
 * screen calls white, the shoulder over what is above the knee, and the codes an
 * SDR frame carries.
 *
 * It is a macro of string literals so each shader source can be one literal
 * with it spliced in, and so a caller can put it inside the #ifdef that needs
 * it: a shader compiled for an SDR screen carries none of this. */
#ifndef YAPFAST_TONE_HLSL_H
#define YAPFAST_TONE_HLSL_H

#define YAPFAST_TONE_HLSL                                                                          \
  "float3 yapfast_srgb_encode(float3 light) {\n"                                                   \
  "  light = max(light, 0.0);\n"                                                                   \
  "  float3 low = light * 12.92;\n"                                                                \
  "  float3 high = 1.055 * pow(light, 1.0 / 2.4) - 0.055;\n"                                       \
  "  return lerp(low, high, step(0.0031308, light));\n"                                            \
  "}\n"                                                                                            \
  "float3 yapfast_tone(float3 colour, float white) {\n"                                            \
  "  float3 light = colour / max(white, 0.001);\n"                                                 \
  "  const float knee = 0.9;\n"                                                                    \
  "  float3 over = max(light - knee, 0.0) / (1.0 - knee);\n"                                       \
  "  light = lerp(light, knee + (1.0 - knee) * (1.0 - exp(-over)), step(knee, light));\n"           \
  "  return yapfast_srgb_encode(light);\n"                                                         \
  "}\n"

#endif /* YAPFAST_TONE_HLSL_H */
