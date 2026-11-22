/* Normalize the private test display to 96 DPI. Weston headless leaves GTK's
 * legacy GDK screen resolution unset (-1), which WebKitGTK interprets literally.
 * This test module has no window and cannot access physical input devices. */
#include <gtk/gtk.h>

void gtk_module_init(int *argc, char ***argv) {
    (void)argc; (void)argv;
    GdkScreen *screen = gdk_screen_get_default();
    if (screen) gdk_screen_set_resolution(screen, 96.0);
}

void gtk_module_display_init(GdkDisplay *display) {
    GdkScreen *screen = gdk_display_get_default_screen(display);
    if (screen) gdk_screen_set_resolution(screen, 96.0);
}
