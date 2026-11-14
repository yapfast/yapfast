/* The step between a capture and an encoder that reads surfaces: Direct3D 11's
 * video processor turning the BGRA texture a capture filled into the NV12
 * texture, at the stream's own size, that Quick Sync and AMF read.
 *
 * It exists because the two never match on their own. A capture hands over BGRA
 * at the screen's or window's size — Desktop Duplication and Windows Graphics
 * Capture both do — while a session whose frames context is a surface reads NV12
 * at the size Discord takes. The video processor is the card's own fixed-function
 * block, so the picture is converted and scaled without a pixel reaching the
 * processor and without an upload: the encoder is handed a texture, not a
 * buffer, and the whole path stays on the device the capture drew on.
 *
 * One texture is made per size and kept: a stream's size changes when the tile it
 * is shown in does, not per frame, so a frame after the first costs one
 * VideoProcessorBlt. */
#ifndef YAPFAST_D3D11CONVERT_H
#define YAPFAST_D3D11CONVERT_H

#include <stddef.h>

struct yapfast_d3d11conv;

/* A converter on `device` (an ID3D11Device), from `from_width` by `from_height`
 * BGRA — or, with `hdr`, the half-float scRGB an HDR screen hands over — to
 * `to_width` by `to_height` NV12. The device is borrowed: the caller keeps it
 * alive for at least as long as this, and nothing here AddRefs it. Null with
 * `why` filled when the device has no video processor, won't take these sizes, or
 * — for an HDR picture — won't take the colour spaces that make this blit a tone
 * map. */
struct yapfast_d3d11conv *yapfast_d3d11conv_new(void *device, unsigned from_width,
                                               unsigned from_height, unsigned to_width,
                                               unsigned to_height, int hdr, char *why,
                                               size_t size);
/* Runs the processor over `texture` and gives back the NV12 texture it wrote,
 * which is this converter's and stays valid until the next call, a resize or
 * [`yapfast_d3d11conv_free`]. Null with `why` filled when the picture wasn't
 * converted. */
void *yapfast_d3d11conv_run(struct yapfast_d3d11conv *conv, void *texture, unsigned subresource,
                            char *why, size_t size);
/* How many pictures the processor converted: what the report says to prove the
 * path stayed on the card. */
unsigned long long yapfast_d3d11conv_converted(struct yapfast_d3d11conv *conv);
/* The one buffer the picture was converted into, for a caller that has to hold
 * the reference it is handed (an encoder that keeps textures AddRefs it). */
unsigned long long yapfast_d3d11conv_copied(struct yapfast_d3d11conv *conv);
const char *yapfast_d3d11conv_error(struct yapfast_d3d11conv *conv);
void yapfast_d3d11conv_free(struct yapfast_d3d11conv *conv);

#endif /* YAPFAST_D3D11CONVERT_H */
