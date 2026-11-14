/* macOS camera enumeration and capture using AVFoundation + libyuv. */
#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "libyuv/convert.h"

struct yapfast_picture {
    int width;
    int height;
    const unsigned char *y;
    const unsigned char *u;
    const unsigned char *v;
    int y_stride;
    int u_stride;
    int v_stride;
};

typedef void (*yapfast_camera_frame)(void *user, const struct yapfast_picture *picture);
typedef void (*yapfast_camera_each)(void *user, const char *name, const char *id);

@interface YapfastCameraDelegate : NSObject <AVCaptureVideoDataOutputSampleBufferDelegate> {
@public
    yapfast_camera_frame _cb;
    void *_user;
    uint8_t *_i420;
    size_t _i420_cap;
    int _stopped;
}
@end

@implementation YapfastCameraDelegate

- (void)dealloc {
    free(_i420);
    _i420 = NULL;
}

- (void)captureOutput:(AVCaptureOutput *)output
didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
       fromConnection:(AVCaptureConnection *)connection {
    (void)output;
    (void)connection;
    if (_stopped || !_cb) return;

    CVImageBufferRef imageBuffer = CMSampleBufferGetImageBuffer(sampleBuffer);
    if (!imageBuffer) return;

    CVPixelBufferLockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
    int w = (int)CVPixelBufferGetWidth(imageBuffer);
    int h = (int)CVPixelBufferGetHeight(imageBuffer);
    OSType fmt = CVPixelBufferGetPixelFormatType(imageBuffer);

    if (w > 0 && h > 0) {
        int uv_w = (w + 1) / 2;
        int uv_h = (h + 1) / 2;
        size_t y_size = (size_t)w * (size_t)h;
        size_t uv_size = (size_t)uv_w * (size_t)uv_h;
        size_t needed = y_size + 2 * uv_size;
        if (needed > _i420_cap) {
            uint8_t *grown = (uint8_t *)realloc(_i420, needed);
            if (!grown) {
                CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
                return;
            }
            _i420 = grown;
            _i420_cap = needed;
        }
        uint8_t *dst_y = _i420;
        uint8_t *dst_u = dst_y + y_size;
        uint8_t *dst_v = dst_u + uv_size;

        int converted = -1;
        if (fmt == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange ||
            fmt == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange) {
            const uint8_t *src_y = (const uint8_t *)CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 0);
            int src_y_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 0);
            const uint8_t *src_uv = (const uint8_t *)CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 1);
            int src_uv_stride = (int)CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 1);
            converted = NV12ToI420(src_y, src_y_stride,
                                   src_uv, src_uv_stride,
                                   dst_y, w,
                                   dst_u, uv_w,
                                   dst_v, uv_w,
                                   w, h);
        } else if (fmt == kCVPixelFormatType_32BGRA) {
            const uint8_t *src = (const uint8_t *)CVPixelBufferGetBaseAddress(imageBuffer);
            int src_stride = (int)CVPixelBufferGetBytesPerRow(imageBuffer);
            converted = ARGBToI420(src, src_stride,
                                   dst_y, w,
                                   dst_u, uv_w,
                                   dst_v, uv_w,
                                   w, h);
        }

        if (converted == 0) {
            struct yapfast_picture pic = {
                .width = w,
                .height = h,
                .y = dst_y,
                .u = dst_u,
                .v = dst_v,
                .y_stride = w,
                .u_stride = uv_w,
                .v_stride = uv_w,
            };
            _cb(_user, &pic);
        }
    }

    CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
}

@end

typedef struct {
    AVCaptureSession *session;
    YapfastCameraDelegate *delegate;
    dispatch_queue_t queue;
} MacCamera;

void yapfast_camera_list(yapfast_camera_each each, void *user) {
    if (!each) return;
    @autoreleasepool {
        NSMutableArray<AVCaptureDeviceType> *types =
            [NSMutableArray arrayWithObject:AVCaptureDeviceTypeBuiltInWideAngleCamera];
        if (@available(macOS 14.0, *)) {
            [types addObject:AVCaptureDeviceTypeExternal];
        } else {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            [types addObject:AVCaptureDeviceTypeExternalUnknown];
#pragma clang diagnostic pop
        }
        AVCaptureDeviceDiscoverySession *discovery =
            [AVCaptureDeviceDiscoverySession discoverySessionWithDeviceTypes:types
                                                                 mediaType:AVMediaTypeVideo
                                                                  position:AVCaptureDevicePositionUnspecified];
        for (AVCaptureDevice *device in discovery.devices) {
            const char *name = [device.localizedName UTF8String];
            const char *uid = [device.uniqueID UTF8String];
            if (name && uid) {
                each(user, name, uid);
            }
        }
    }
}

void *yapfast_camera_open(const char *id, int width, int height, int fps,
                          yapfast_camera_frame frame, void *user) {
    if (!id || !frame) return NULL;
    (void)width;
    (void)height;
    (void)fps;
    @autoreleasepool {
        NSString *uid = [NSString stringWithUTF8String:id];
        AVCaptureDevice *device = [AVCaptureDevice deviceWithUniqueID:uid];
        if (!device) return NULL;

        NSError *error = nil;
        AVCaptureDeviceInput *input = [AVCaptureDeviceInput deviceInputWithDevice:device error:&error];
        if (!input || error) return NULL;

        AVCaptureSession *session = [[AVCaptureSession alloc] init];
        if ([session canSetSessionPreset:AVCaptureSessionPreset1280x720]) {
            session.sessionPreset = AVCaptureSessionPreset1280x720;
        }
        if (![session canAddInput:input]) return NULL;
        [session addInput:input];

        AVCaptureVideoDataOutput *output = [[AVCaptureVideoDataOutput alloc] init];
        output.alwaysDiscardsLateVideoFrames = YES;
        output.videoSettings = @{
            (NSString *)kCVPixelBufferPixelFormatTypeKey :
                @(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
        };
        if (![session canAddOutput:output]) return NULL;
        [session addOutput:output];

        YapfastCameraDelegate *delegate = [[YapfastCameraDelegate alloc] init];
        delegate->_cb = frame;
        delegate->_user = user;
        delegate->_stopped = 0;

        dispatch_queue_t queue = dispatch_queue_create("io.github.yapfast.camera", DISPATCH_QUEUE_SERIAL);
        [output setSampleBufferDelegate:delegate queue:queue];

        [session startRunning];
        if (!session.isRunning) {
            return NULL;
        }

        MacCamera *cam = (MacCamera *)calloc(1, sizeof(MacCamera));
        cam->session = session;
        cam->delegate = delegate;
        cam->queue = queue;
        return cam;
    }
}

void yapfast_camera_close(void *raw) {
    MacCamera *cam = (MacCamera *)raw;
    if (!cam) return;
    @autoreleasepool {
        cam->delegate->_stopped = 1;
        [cam->session stopRunning];
        dispatch_sync(cam->queue, ^{});
        cam->session = nil;
        cam->delegate = nil;
        cam->queue = nil;
        free(cam);
    }
}
