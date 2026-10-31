// The parts of being a Mac application that Slint and winit don't do: the
// menu bar, an icon in the menu bar's right side, notifications, starting at
// sign-in, and what a copy from Finder holds.
//
// Everything here runs on the main thread but for notifications, which macOS
// delivers on a queue of their own; Rust marshals what comes back with
// slint::invoke_from_event_loop. Yapfast calls each function from Slint's own
// event loop, which is the main thread.

#import <AppKit/AppKit.h>
#import <ServiceManagement/ServiceManagement.h>
#import <UserNotifications/UserNotifications.h>
#include <stdint.h>
#include <string.h>

/// What the menu bar, the tray or the Dock was asked for. The same numbers, so
/// that one Rust function answers all three.
enum {
  YAP_OPEN = 0,
  YAP_QUIT = 1,
  YAP_DISCONNECT = 2,
  YAP_SETTINGS = 3,
  YAP_REOPEN = 4,
};

typedef void (*YapChoice)(int32_t choice);
typedef void (*YapOpened)(const char *room);

static YapChoice menu_chosen = NULL;
static YapChoice tray_chosen = NULL;
/// The conversation each notification shown belongs to, by its identifier.
static NSMutableDictionary<NSString *, NSString *> *notification_rooms = nil;
static YapOpened notification_opened = NULL;

// --- the menu bar ---

/// The target of the menu items Yapfast answers for itself. The rest name
/// AppKit's own selectors, so the standard keys do what every other Mac app's
/// do without Yapfast deciding each one.
@interface YapCommands : NSObject
- (void)yapOpen:(__unused id)sender;
- (void)yapQuit:(__unused id)sender;
- (void)yapSettings:(__unused id)sender;
@end

@implementation YapCommands
- (void)yapOpen:(__unused id)sender {
  if (menu_chosen != NULL) {
    menu_chosen(YAP_OPEN);
  }
}
- (void)yapQuit:(__unused id)sender {
  if (menu_chosen != NULL) {
    menu_chosen(YAP_QUIT);
  }
}
- (void)yapSettings:(__unused id)sender {
  if (menu_chosen != NULL) {
    menu_chosen(YAP_SETTINGS);
  }
}
@end

static YapCommands *commands = nil;

/// The bar itself. `NSApplication`'s `mainMenu`, `servicesMenu` and
/// `windowsMenu` are all `assign` properties, from when a nib owned them, so
/// Yapfast has to keep the menu alive itself rather than let it go at the end
/// of `install_menu`.
static NSMenu *main_bar = nil;

/// One item of a menu, aimed at Yapfast's own answers.
static NSMenuItem *own(NSString *title, SEL action, NSString *key) {
  NSMenuItem *entry = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:key];
  [entry setTarget:commands];
  return entry;
}

/// One item aimed at whoever has the keyboard, which is how Cut, Copy and the
/// rest reach the text field Slint has the focus in.
static NSMenuItem *responder(NSString *title, SEL action, NSString *key) {
  return [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:key];
}

static void install_menu(void) {
  NSString *name = @"Yapfast";
  NSMenu *bar = [[NSMenu alloc] init];

  // The bar shows each item's own title, not its submenu's, so each of these
  // three carries the name it should be read by.
  NSMenuItem *application = [[NSMenuItem alloc] initWithTitle:name action:NULL keyEquivalent:@""];
  [bar addItem:application];
  NSMenu *application_menu = [[NSMenu alloc] initWithTitle:name];
  [application_menu addItem:responder([NSString stringWithFormat:@"About %@", name],
                                      @selector(orderFrontStandardAboutPanel:), @"")];
  [application_menu addItem:[NSMenuItem separatorItem]];
  [application_menu addItem:own(@"Settings…", @selector(yapSettings:), @",")];
  [application_menu addItem:[NSMenuItem separatorItem]];

  NSMenu *services = [[NSMenu alloc] initWithTitle:@"Services"];
  [NSApp setServicesMenu:services];
  NSMenuItem *services_item = [[NSMenuItem alloc] initWithTitle:@"Services" action:NULL
                                                  keyEquivalent:@""];
  [services_item setSubmenu:services];
  [application_menu addItem:services_item];
  [application_menu addItem:[NSMenuItem separatorItem]];

  [application_menu addItem:responder([NSString stringWithFormat:@"Hide %@", name],
                                      @selector(hide:), @"h")];
  NSMenuItem *others = responder(@"Hide Others", @selector(hideOtherApplications:), @"h");
  [others setKeyEquivalentModifierMask:NSEventModifierFlagOption | NSEventModifierFlagCommand];
  [application_menu addItem:others];
  [application_menu addItem:responder(@"Show All", @selector(unhideAllApplications:), @"")];
  [application_menu addItem:[NSMenuItem separatorItem]];
  // Yapfast's own rather than AppKit's `terminate:`, so that quitting leaves
  // the session, the log and the tray the way closing the window does.
  [application_menu addItem:own([NSString stringWithFormat:@"Quit %@", name],
                                @selector(yapQuit:), @"q")];
  [application setSubmenu:application_menu];

  NSMenuItem *edit = [[NSMenuItem alloc] initWithTitle:@"Edit" action:NULL keyEquivalent:@""];
  [bar addItem:edit];
  NSMenu *edit_menu = [[NSMenu alloc] initWithTitle:@"Edit"];
  [edit_menu addItem:responder(@"Undo", @selector(undo:), @"z")];
  [edit_menu addItem:responder(@"Redo", @selector(redo:), @"Z")];
  [edit_menu addItem:[NSMenuItem separatorItem]];
  [edit_menu addItem:responder(@"Cut", @selector(cut:), @"x")];
  [edit_menu addItem:responder(@"Copy", @selector(copy:), @"c")];
  [edit_menu addItem:responder(@"Paste", @selector(paste:), @"v")];
  [edit_menu addItem:responder(@"Select All", @selector(selectAll:), @"a")];
  [edit setSubmenu:edit_menu];

  NSMenuItem *window_item = [[NSMenuItem alloc] initWithTitle:@"Window" action:NULL keyEquivalent:@""];
  [bar addItem:window_item];
  NSMenu *window_menu = [[NSMenu alloc] initWithTitle:@"Window"];
  [window_menu addItem:responder(@"Minimise", @selector(performMiniaturize:), @"m")];
  [window_menu addItem:responder(@"Zoom", @selector(performZoom:), @"")];
  [window_menu addItem:[NSMenuItem separatorItem]];
  [window_menu addItem:responder(@"Bring All to Front", @selector(arrangeInFront:), @"")];
  [NSApp setWindowsMenu:window_menu];
  [window_item setSubmenu:window_menu];

  [NSApp setMainMenu:bar];
  main_bar = bar;
}

/// Asks for the window when the application is activated with none showing,
/// which is what closing Yapfast to the menu bar and then clicking its Dock
/// icon means. Reopen rather than Open, because a Yapfast that started in the
/// menu bar, as starting at sign-in does, has never had one to bring back.
static void watch_reopen(void) {
  [[NSNotificationCenter defaultCenter]
      addObserverForName:NSApplicationDidBecomeActiveNotification
                  object:nil
                   queue:[NSOperationQueue mainQueue]
              usingBlock:^(__unused NSNotification *note) {
                for (NSWindow *window in [NSApp windows]) {
                  if ([window isVisible]) {
                    return; // one of them is up; there's nothing to reopen
                  }
                }
                if (menu_chosen != NULL) {
                  menu_chosen(YAP_REOPEN);
                }
              }];
}

void yap_app_install(YapChoice chosen) {
  menu_chosen = chosen;
  commands = [[YapCommands alloc] init];
  [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
  install_menu();
  watch_reopen();
}

// --- the menu bar's right side ---

static NSStatusItem *status = nil;
/// The tray's menu. Held here rather than given to the status item, because an
/// item with a menu of its own opens it on every click, and the left one is
/// spent opening the window.
static NSMenu *tray_popup = nil;

/// The tray's own answers: its icon opens the window, and its menu offers the
/// same three things the Linux one does.
@interface YapTray : NSObject
- (void)yapClicked:(__unused id)sender;
- (void)yapOpen:(__unused id)sender;
- (void)yapDisconnect:(__unused id)sender;
- (void)yapQuit:(__unused id)sender;
@end

@implementation YapTray
- (void)choose:(int32_t)choice {
  if (tray_chosen != NULL) {
    tray_chosen(choice);
  }
}
- (void)yapClicked:(__unused id)sender {
  NSEvent *event = [NSApp currentEvent];
  int right = [event type] == NSEventTypeRightMouseUp ||
              ([event modifierFlags] & NSEventModifierFlagControl) != 0;
  if (right && tray_popup != nil) {
    // Deprecated, but the one call that puts the menu under the item, and
    // Apple has never taken it away. Popping it by hand is what keeps the
    // left click for the window.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    [status popUpStatusItemMenu:tray_popup];
#pragma clang diagnostic pop
    return;
  }
  [self choose:YAP_OPEN];
}
- (void)yapOpen:(__unused id)sender {
  [self choose:YAP_OPEN];
}
- (void)yapDisconnect:(__unused id)sender {
  [self choose:YAP_DISCONNECT];
}
- (void)yapQuit:(__unused id)sender {
  [self choose:YAP_QUIT];
}
@end

static YapTray *tray = nil;

/// One tray icon: RGBA, `side` by `side`, from Rust's drawing of the logo.
static NSBitmapImageRep *bitmap(__unused const uint8_t *pixels, uint32_t width, uint32_t height) {
  return [[NSBitmapImageRep alloc]
      initWithBitmapDataPlanes:nil
                    pixelsWide:(NSInteger)width
                    pixelsHigh:(NSInteger)height
                 bitsPerSample:8
              samplesPerPixel:4
                     hasAlpha:YES
                     isPlanar:NO
               colorSpaceName:NSDeviceRGBColorSpace
                  bytesPerRow:(NSInteger)width * 4
                 bitsPerPixel:32];
}

/// The logo as the menu bar draws it: one colour, following whether the bar is
/// light or dark. A template image keeps only its alpha, so the three dark dots
/// are punched out of the marigold shape rather than lost inside it.
static NSImage *menu_bar_image(const uint8_t *pixels, uint32_t side) {
  NSBitmapImageRep *rep = bitmap(pixels, side, side);
  if (rep == nil) {
    return nil;
  }
  uint8_t *out = [rep bitmapData];
  for (uint32_t at = 0; at < side * side; at++) {
    const uint8_t *pixel = pixels + at * 4;
    uint32_t shade = ((uint32_t)pixel[0] + pixel[1] + pixel[2]) / 3;
    out[at * 4 + 0] = 0;
    out[at * 4 + 1] = 0;
    out[at * 4 + 2] = 0;
    out[at * 4 + 3] = shade < 96 ? 0 : pixel[3];
  }
  NSImage *image = [[NSImage alloc] init];
  [image addRepresentation:rep];
  // The buffer comes at twice the size the bar draws it, so it stays crisp.
  image.size = NSMakeSize(side / 2.0, side / 2.0);
  image.template = YES;
  return image;
}

/// One item of the tray's menu.
static NSMenuItem *tray_item(NSString *title, SEL action) {
  NSMenuItem *entry = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:@""];
  [entry setTarget:tray];
  return entry;
}

/// The tray's menu: what the other desktops' trays hold, with Disconnect only
/// while a call is up. Made again whenever the icon's state changes.
static NSMenu *tray_menu(int in_call) {
  NSMenu *menu = [[NSMenu alloc] init];
  [menu addItem:tray_item(@"Open Yapfast", @selector(yapOpen:))];
  if (in_call) {
    [menu addItem:tray_item(@"Disconnect", @selector(yapDisconnect:))];
  }
  [menu addItem:[NSMenuItem separatorItem]];
  [menu addItem:tray_item(@"Quit", @selector(yapQuit:))];
  return menu;
}

void yap_tray_start(YapChoice chosen, const uint8_t *pixels, uint32_t side) {
  tray_chosen = chosen;
  tray = [[YapTray alloc] init];
  status = [[NSStatusBar systemStatusBar] statusItemWithLength:NSVariableStatusItemLength];
  NSImage *image = menu_bar_image(pixels, side);
  if (image != nil) {
    [[status button] setImage:image];
  }
  [[status button] setTarget:tray];
  [[status button] setAction:@selector(yapClicked:)];
  [[status button] setToolTip:@"Yapfast"];
  tray_popup = tray_menu(0);
  // Both clicks come to the action, which tells them apart: left opens the
  // window, right shows the menu the other desktops' trays hold.
  [[status button] sendActionOn:NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp];
}

/// The unread dot and the call's Disconnect, which are all it ever shows.
void yap_tray_show(const uint8_t *pixels, uint32_t side, int in_call) {
  if (status == nil) {
    return;
  }
  NSImage *image = menu_bar_image(pixels, side);
  if (image != nil) {
    [[status button] setImage:image];
  }
  [[status button] setToolTip:in_call ? @"Yapfast — in a call" : @"Yapfast"];
  tray_popup = tray_menu(in_call);
}

int yap_tray_available(void) { return status != nil && [status isVisible] ? 1 : 0; }

void yap_tray_stop(void) {
  if (status == nil) {
    return;
  }
  [[NSStatusBar systemStatusBar] removeStatusItem:status];
  status = nil;
}

// --- notifications ---

/// Hands the conversation of a clicked notification back to Yapfast.
@interface YapNotifications : NSObject <UNUserNotificationCenterDelegate>
@end

@implementation YapNotifications

- (void)userNotificationCenter:(__unused UNUserNotificationCenter *)center
       willPresentNotification:(__unused UNNotification *)notification
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completion {
  // A banner only: Yapfast plays its own sounds and rings for a call itself.
  completion(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionList);
}

- (void)userNotificationCenter:(__unused UNUserNotificationCenter *)center
    didReceiveNotificationResponse:(UNNotificationResponse *)response
             withCompletionHandler:(void (^)(void))completion {
  NSString *room = nil;
  @synchronized(notification_rooms) {
    room = notification_rooms[response.notification.request.identifier];
    [notification_rooms removeObjectForKey:response.notification.request.identifier];
  }
  if (room != nil && notification_opened != NULL) {
    notification_opened([room UTF8String]);
  }
  completion();
}

@end

static YapNotifications *notifications = nil;

/// Whether this process can use the notification service at all: it wants a
/// bundle with an identifier, which a bare `cargo run` binary doesn't have.
static int notifications_available(void) {
  static int checked = -1;
  if (checked < 0) {
    checked = [[[NSBundle mainBundle] bundleIdentifier] length] > 0;
  }
  return checked;
}

void yap_notifications_start(YapOpened opened) {
  if (!notifications_available()) {
    return;
  }
  notification_opened = opened;
  notification_rooms = [NSMutableDictionary dictionary];
  notifications = [[YapNotifications alloc] init];
  UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
  [center setDelegate:notifications];
  [center requestAuthorizationWithOptions:(UNAuthorizationOptionAlert)
                        completionHandler:^(__unused BOOL granted, __unused NSError *error) {}];
}

/// Counts the pictures written to the temporary folder, and the notifications,
/// so each gets a name of its own.
static uint64_t shown = 0;

/// Shows one notification. `picture` is the author's RGBA and goes along as a
/// PNG the system copies, or is null.
void yap_notify_show(const char *identifier, const char *title, const char *body,
                     const uint8_t *picture, uint32_t width, uint32_t height) {
  if (!notifications_available()) {
    return;
  }
  UNMutableNotificationContent *content = [[UNMutableNotificationContent alloc] init];
  content.title = [NSString stringWithUTF8String:title];
  content.body = [NSString stringWithUTF8String:body];
  content.sound = nil;

  NSString *room = [NSString stringWithUTF8String:identifier];
  if (picture != NULL && width > 0 && height > 0) {
    NSBitmapImageRep *rep = bitmap(picture, width, height);
    NSData *png = nil;
    if (rep != nil) {
      memcpy([rep bitmapData], picture, (size_t)width * height * 4);
      png = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
    }
    if (png != nil) {
      NSString *path = [NSTemporaryDirectory()
          stringByAppendingFormat:@"yapfast-%llu.png", (unsigned long long)(++shown)];
      if ([png writeToFile:path atomically:YES]) {
        UNNotificationAttachment *attachment = [UNNotificationAttachment
            attachmentWithIdentifier:@"picture"
                                 URL:[NSURL fileURLWithPath:path]
                             options:nil
                               error:NULL];
        if (attachment != nil) {
          content.attachments = @[ attachment ];
        }
      }
    }
  }

  NSString *identifier_for_request =
      [NSString stringWithFormat:@"%@-%llu", room, (unsigned long long)(++shown)];
  UNNotificationRequest *request =
      [UNNotificationRequest requestWithIdentifier:identifier_for_request content:content
                                          trigger:nil];
  @synchronized(notification_rooms) {
    notification_rooms[identifier_for_request] = room;
  }
  [[UNUserNotificationCenter currentNotificationCenter]
      addNotificationRequest:request
       withCompletionHandler:^(__unused NSError *error) {}];
}

// --- starting at sign-in ---

/// Whether this macOS can register a login item at all, which decides whether
/// Settings offers it.
int yap_login_item_supported(void) {
  if (@available(macOS 13.0, *)) {
    return 1;
  }
  return 0;
}

/// Whether Yapfast is a login item. macOS 13 and later only; before that this
/// answers no.
int yap_login_item_enabled(void) {
  if (@available(macOS 13.0, *)) {
    return SMAppService.mainAppService.status == SMAppServiceStatusEnabled ? 1 : 0;
  }
  return 0;
}

/// Registers or unregisters Yapfast as a login item. Registering takes the
/// person to System Settings to agree, which is why the setting says so.
int yap_login_item_set(int wanted, char *err, size_t err_length) {
  if (@available(macOS 13.0, *)) {
    NSError *error = nil;
    BOOL done = wanted ? [SMAppService.mainAppService registerAndReturnError:&error]
                       : [SMAppService.mainAppService unregisterAndReturnError:&error];
    if (!done && err != NULL && err_length > 0) {
      snprintf(err, err_length, "%s",
               error.localizedDescription.UTF8String ?: "macOS refused");
    }
    return done ? 1 : 0;
  }
  if (err != NULL && err_length > 0) {
    snprintf(err, err_length, "macOS 13 or later is needed to start at sign-in");
  }
  return 0;
}

// --- what a copy from Finder holds ---

/// The files on the clipboard, as their paths one to a line with an empty line
/// at the end. Returns how many there were, or -1 when they didn't fit.
int32_t yap_pasteboard_files(char *out, size_t length) {
  NSArray<NSURL *> *urls =
      [[NSPasteboard generalPasteboard]
          readObjectsForClasses:@[ [NSURL class] ]
                        options:@{NSPasteboardURLReadingFileURLsOnlyKey : @YES}];
  if (urls.count == 0) {
    return 0;
  }
  size_t used = 0;
  for (NSURL *url in urls) {
    const char *path = [[url path] UTF8String];
    if (path == NULL) {
      continue;
    }
    size_t need = strlen(path) + 1;
    if (used + need > length) {
      return -1;
    }
    memcpy(out + used, path, need);
    used += need;
  }
  out[used++] = '\0';
  return (int32_t)urls.count;
}
