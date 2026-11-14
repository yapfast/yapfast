#import <AppKit/AppKit.h>

/* Hides the `--stream-worker` subprocess from the macOS Dock so starting a
 * stream or opening the share menu does not spawn a bouncing duplicate icon. */
void yapfast_macos_hide_worker_from_dock(void) {
  @autoreleasepool {
    NSApplication *app = [NSApplication sharedApplication];
    [app setActivationPolicy:NSApplicationActivationPolicyProhibited];
  }
}
