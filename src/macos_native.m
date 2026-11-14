/* macOS desktop integration: NSStatusItem tray, notifications, and CGEventTap push-to-talk. */
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>
#include <dispatch/dispatch.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* ---------------- Tray (NSStatusItem) ---------------- */

typedef void (*yapfast_tray_cb)(int choice);

@interface YapfastTrayTarget : NSObject {
@public
    NSStatusItem *_item;
    NSMenuItem *_disconnectItem;
    yapfast_tray_cb _cb;
}
- (void)onOpen:(id)sender;
- (void)onDisconnect:(id)sender;
- (void)onQuit:(id)sender;
@end

@implementation YapfastTrayTarget
- (void)onOpen:(id)sender {
    (void)sender;
    if (_cb) _cb(0);
}
- (void)onDisconnect:(id)sender {
    (void)sender;
    if (_cb) _cb(1);
}
- (void)onQuit:(id)sender {
    (void)sender;
    if (_cb) _cb(2);
}
@end

static YapfastTrayTarget *g_tray = nil;

int yapfast_macos_tray_start(const uint8_t *rgba, int width, int height, yapfast_tray_cb cb) {
    if (!cb || !rgba || width <= 0 || height <= 0) return -1;
    NSData *imgData = [NSData dataWithBytes:rgba length:(NSUInteger)width * (NSUInteger)height * 4];
    dispatch_async(dispatch_get_main_queue(), ^{
        if (g_tray) return;
        NSStatusBar *bar = [NSStatusBar systemStatusBar];
        NSStatusItem *item = [bar statusItemWithLength:NSSquareStatusItemLength];
        if (!item) return;

        YapfastTrayTarget *target = [[YapfastTrayTarget alloc] init];
        target->_item = item;
        target->_cb = cb;

        NSMenu *menu = [[NSMenu alloc] initWithTitle:@"Yapfast"];
        NSMenuItem *openItem = [[NSMenuItem alloc] initWithTitle:@"Open Yapfast"
                                                         action:@selector(onOpen:)
                                                  keyEquivalent:@""];
        openItem.target = target;
        [menu addItem:openItem];

        NSMenuItem *discItem = [[NSMenuItem alloc] initWithTitle:@"Disconnect"
                                                         action:@selector(onDisconnect:)
                                                  keyEquivalent:@""];
        discItem.target = target;
        discItem.hidden = YES;
        [menu addItem:discItem];
        target->_disconnectItem = discItem;

        [menu addItem:[NSMenuItem separatorItem]];

        NSMenuItem *quitItem = [[NSMenuItem alloc] initWithTitle:@"Quit"
                                                         action:@selector(onQuit:)
                                                  keyEquivalent:@"q"];
        quitItem.target = target;
        [menu addItem:quitItem];

        item.menu = menu;

        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL
                          pixelsWide:width
                          pixelsHigh:height
                       bitsPerSample:8
                     samplesPerPixel:4
                            hasAlpha:YES
                            isPlanar:NO
                      colorSpaceName:NSCalibratedRGBColorSpace
                         bytesPerRow:width * 4
                        bitsPerPixel:32];
        if (rep && imgData.length == (NSUInteger)(width * height * 4)) {
            memcpy(rep.bitmapData, imgData.bytes, imgData.length);
            NSImage *img = [[NSImage alloc] initWithSize:NSMakeSize(18, 18)];
            [img addRepresentation:rep];
            item.button.image = img;
        }
        g_tray = target;
    });
    return 0;
}

void yapfast_macos_tray_update(const uint8_t *rgba, int width, int height, int in_call) {
    NSData *imgData = (rgba && width > 0 && height > 0)
        ? [NSData dataWithBytes:rgba length:(NSUInteger)width * (NSUInteger)height * 4]
        : nil;
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!g_tray) return;
        g_tray->_disconnectItem.hidden = !in_call;
        if (imgData && imgData.length == (NSUInteger)(width * height * 4)) {
            NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
                initWithBitmapDataPlanes:NULL
                              pixelsWide:width
                              pixelsHigh:height
                           bitsPerSample:8
                         samplesPerPixel:4
                                hasAlpha:YES
                                isPlanar:NO
                          colorSpaceName:NSCalibratedRGBColorSpace
                             bytesPerRow:width * 4
                            bitsPerPixel:32];
            if (rep) {
                memcpy(rep.bitmapData, imgData.bytes, imgData.length);
                NSImage *img = [[NSImage alloc] initWithSize:NSMakeSize(18, 18)];
                [img addRepresentation:rep];
                g_tray->_item.button.image = img;
            }
        }
    });
}

void yapfast_macos_tray_stop(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!g_tray) return;
        [[NSStatusBar systemStatusBar] removeStatusItem:g_tray->_item];
        g_tray = nil;
    });
}

/* ---------------- Notifications ---------------- */

typedef void (*yapfast_notify_opened_cb)(const char *conversation);

@interface YapfastNotifyDelegate : NSObject <UNUserNotificationCenterDelegate> {
@public
    yapfast_notify_opened_cb _cb;
}
@end

@implementation YapfastNotifyDelegate
- (void)userNotificationCenter:(UNUserNotificationCenter *)center
       willPresentNotification:(UNNotification *)notification
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completionHandler {
    (void)center;
    (void)notification;
    if (@available(macOS 11.0, *)) {
        completionHandler(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionSound);
    } else {
        completionHandler(UNNotificationPresentationOptionSound);
    }
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
didReceiveNotificationResponse:(UNNotificationResponse *)response
         withCompletionHandler:(void (^)(void))completionHandler {
    (void)center;
    NSString *conv = response.notification.request.content.userInfo[@"conversation"];
    if (conv && _cb) {
        _cb([conv UTF8String]);
    }
    completionHandler();
}
@end

static YapfastNotifyDelegate *g_notify_delegate = nil;
static int g_notify_has_bundle = 0;

void yapfast_macos_notify_init(yapfast_notify_opened_cb cb) {
    @autoreleasepool {
        NSString *bundleId = [[NSBundle mainBundle] bundleIdentifier];
        if (!bundleId || bundleId.length == 0) {
            g_notify_has_bundle = 0;
            return;
        }
        g_notify_has_bundle = 1;
        g_notify_delegate = [[YapfastNotifyDelegate alloc] init];
        g_notify_delegate->_cb = cb;
        UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
        center.delegate = g_notify_delegate;
        [center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                              completionHandler:^(BOOL granted, NSError *error) {
            (void)granted;
            (void)error;
        }];
    }
}

void yapfast_macos_notify_show(const char *title, const char *subtitle, const char *body,
                               const char *conversation) {
    @autoreleasepool {
        NSString *nsTitle = title ? [NSString stringWithUTF8String:title] : @"Yapfast";
        NSString *nsSub = subtitle ? [NSString stringWithUTF8String:subtitle] : @"";
        NSString *nsBody = body ? [NSString stringWithUTF8String:body] : @"";
        NSString *nsConv = conversation ? [NSString stringWithUTF8String:conversation] : @"";

        if (g_notify_has_bundle) {
            UNMutableNotificationContent *content = [[UNMutableNotificationContent alloc] init];
            content.title = nsTitle;
            if (nsSub.length > 0) content.subtitle = nsSub;
            content.body = nsBody;
            content.sound = [UNNotificationSound defaultSound];
            content.userInfo = @{@"conversation" : nsConv};
            NSString *reqId = [[NSUUID UUID] UUIDString];
            UNNotificationRequest *req = [UNNotificationRequest requestWithIdentifier:reqId
                                                                              content:content
                                                                              trigger:nil];
            [[UNUserNotificationCenter currentNotificationCenter] addNotificationRequest:req
                                                                   withCompletionHandler:nil];
        } else {
            /* Unbundled CLI/cargo-run fallback via osascript. */
            NSString *escapedBody = [nsBody stringByReplacingOccurrencesOfString:@"\"" withString:@"\\\""];
            NSString *escapedTitle = [nsTitle stringByReplacingOccurrencesOfString:@"\"" withString:@"\\\""];
            NSString *script = [NSString stringWithFormat:@"display notification \"%@\" with title \"%@\"",
                                                          escapedBody, escapedTitle];
            NSTask *task = [[NSTask alloc] init];
            task.launchPath = @"/usr/bin/osascript";
            task.arguments = @[@"-e", script];
            @try {
                [task launch];
            } @catch (NSException *ex) {
                (void)ex;
            }
        }
    }
}

/* ---------------- Push-to-Talk (CGEventTap) ---------------- */

typedef void (*yapfast_ptt_cb)(void *user, uint32_t scancode, int down);
typedef void (*yapfast_ptt_ready_cb)(void *user, void *handle);

typedef struct {
    CFMachPortRef tap;
    CFRunLoopRef run_loop;
    yapfast_ptt_cb cb;
    void *user;
    uint64_t last_flags;
} MacPttState;

static const uint32_t MAC_MOUSE_4 = 0xFFFF0004u;
static const uint32_t MAC_MOUSE_5 = 0xFFFF0005u;

static uint64_t flag_for_keycode(int64_t keycode) {
    switch (keycode) {
        case 56: case 60: return kCGEventFlagMaskShift;     /* Left/Right Shift */
        case 59: case 62: return kCGEventFlagMaskControl;   /* Left/Right Control */
        case 58: case 61: return kCGEventFlagMaskAlternate; /* Left/Right Option */
        case 55: case 54: return kCGEventFlagMaskCommand;   /* Left/Right Command */
        case 57:          return kCGEventFlagMaskAlphaShift;/* Caps Lock */
        default:          return 0;
    }
}

static CGEventRef ptt_event_cb(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *refcon) {
    (void)proxy;
    MacPttState *st = (MacPttState *)refcon;
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        if (st->tap) CGEventTapEnable(st->tap, true);
        return event;
    }
    if (!st->cb) return event;

    if (type == kCGEventKeyDown || type == kCGEventKeyUp) {
        if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat) != 0) {
            return event;
        }
        uint32_t code = (uint32_t)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
        st->cb(st->user, code, type == kCGEventKeyDown ? 1 : 0);
    } else if (type == kCGEventFlagsChanged) {
        int64_t code = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
        uint64_t flags = (uint64_t)CGEventGetFlags(event);
        uint64_t mask = flag_for_keycode(code);
        if (mask != 0) {
            int was_down = (st->last_flags & mask) != 0;
            int is_down = (flags & mask) != 0;
            if (was_down != is_down) {
                st->cb(st->user, (uint32_t)code, is_down ? 1 : 0);
            }
        }
        st->last_flags = flags;
    } else if (type == kCGEventOtherMouseDown || type == kCGEventOtherMouseUp) {
        int64_t btn = CGEventGetIntegerValueField(event, kCGMouseEventButtonNumber);
        int down = (type == kCGEventOtherMouseDown) ? 1 : 0;
        if (btn == 3) st->cb(st->user, MAC_MOUSE_4, down);
        else if (btn == 4) st->cb(st->user, MAC_MOUSE_5, down);
    }
    return event;
}

int yapfast_macos_ptt_run(yapfast_ptt_cb cb, yapfast_ptt_ready_cb ready, void *user) {
    CGEventMask mask = CGEventMaskBit(kCGEventKeyDown) |
                       CGEventMaskBit(kCGEventKeyUp) |
                       CGEventMaskBit(kCGEventFlagsChanged) |
                       CGEventMaskBit(kCGEventOtherMouseDown) |
                       CGEventMaskBit(kCGEventOtherMouseUp);

    MacPttState *st = (MacPttState *)calloc(1, sizeof(MacPttState));
    if (!st) return -1;
    st->cb = cb;
    st->user = user;

    CFMachPortRef tap = CGEventTapCreate(kCGSessionEventTap,
                                         kCGHeadInsertEventTap,
                                         kCGEventTapOptionListenOnly,
                                         mask,
                                         ptt_event_cb,
                                         st);
    if (!tap) {
        free(st);
        return -1;
    }
    st->tap = tap;
    st->run_loop = CFRunLoopGetCurrent();
    if (ready) ready(user, st);

    CFRunLoopSourceRef src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
    CFRunLoopAddSource(st->run_loop, src, kCFRunLoopCommonModes);
    CGEventTapEnable(tap, true);
    CFRunLoopRun();

    CFRunLoopRemoveSource(st->run_loop, src, kCFRunLoopCommonModes);
    CFRelease(src);
    CFRelease(tap);
    free(st);
    return 0;
}

void yapfast_macos_ptt_stop(void *handle) {
    MacPttState *st = (MacPttState *)handle;
    if (!st || !st->run_loop) return;
    CFRunLoopStop(st->run_loop);
}
