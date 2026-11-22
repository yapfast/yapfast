/* Test-only virtual seat for Weston 14. Never opens a physical input device.
 * These exported internal functions are pinned to Weston 14.0.1. */
#include <libweston/libweston.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <time.h>
#include <errno.h>
#include <inttypes.h>

extern void weston_seat_init(struct weston_seat *, struct weston_compositor *, const char *);
extern int weston_seat_init_keyboard(struct weston_seat *, struct xkb_keymap *);
extern int weston_seat_init_pointer(struct weston_seat *);
extern void weston_seat_release(struct weston_seat *);
extern void notify_motion_absolute(struct weston_seat *, const struct timespec *, struct weston_coord_global);
extern void notify_button(struct weston_seat *, const struct timespec *, int32_t, enum wl_pointer_button_state);
extern void notify_pointer_frame(struct weston_seat *);
extern void notify_axis(struct weston_seat *, const struct timespec *, struct weston_pointer_axis_event *);
extern void notify_axis_source(struct weston_seat *, uint32_t);
extern void notify_key(struct weston_seat *, const struct timespec *, uint32_t, enum wl_keyboard_key_state, enum weston_key_state_update);

static struct weston_seat seat;
static struct wl_listener cleanup;
static struct wl_event_source *input_source;
static int input_fd = -1;
static struct wl_protocol_logger *surface_logger;
static FILE *surface_log;

/* Opt-in client update telemetry, never actual presented FPS.
 * libwayland 1.24 log_closure() runs before wl_closure_invoke()/dispatch()
 * in src/wayland-server.c. At wl_surface.commit, Weston's pending state
 * still describes the submitted attach/damage, including synchronized
 * subsurfaces whose state will be cached rather than applied immediately.
 * Current dimensions belong to the state before this request is applied.
 * A buffer submission can be superseded, unchanged, hidden, or coalesced.
 * Weston 14's output.frame_signal also is not presentation feedback: its
 * GL renderer emits that signal before synchronization and buffer swap.
 * API: https://wayland.freedesktop.org/docs/html/apc.html
 */
static void log_surface_commit(void *data, enum wl_protocol_logger_type direction,
                               const struct wl_protocol_logger_message *message) {
    if (direction != WL_PROTOCOL_LOGGER_REQUEST ||
        strcmp(wl_resource_get_class(message->resource), "wl_surface") != 0 ||
        strcmp(message->message->name, "commit") != 0)
        return;

    struct weston_surface *surface = wl_resource_get_user_data(message->resource);
    if (!surface || surface->resource != message->resource || surface->compositor != data)
        return;

    pid_t pid;
    wl_client_get_credentials(wl_resource_get_client(message->resource), &pid, NULL, NULL);
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) < 0)
        return;
    uint64_t nanoseconds = (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
    bool attached = (surface->pending.status & WESTON_SURFACE_DIRTY_BUFFER) != 0;
    bool new_buffer = attached && surface->pending.buffer != NULL;
    bool has_buffer = attached ? surface->pending.buffer != NULL : surface->buffer_ref.buffer != NULL;
    bool damaged = pixman_region32_not_empty(&surface->pending.damage_surface) ||
                   pixman_region32_not_empty(&surface->pending.damage_buffer);
    struct weston_surface *main = weston_surface_get_main_surface(surface);
    uint32_t main_id = main && main->resource ? wl_resource_get_id(main->resource) : 0;

    fprintf(surface_log,
        "{\"event\":\"wl_surface.commit\",\"monotonic_ns\":%" PRIu64
        ",\"pid\":%d,\"surface_id\":%" PRIu32 ",\"main_surface_id\":%" PRIu32
        ",\"width_current\":%d,\"height_current\":%d,\"buffer_attached\":%s"
        ",\"new_buffer\":%s,\"has_buffer\":%s,\"damaged\":%s}\n",
        nanoseconds, (int)pid, wl_resource_get_id(message->resource), main_id,
        surface->width, surface->height, attached ? "true" : "false",
        new_buffer ? "true" : "false", has_buffer ? "true" : "false", damaged ? "true" : "false");
}

static int input_event(int fd, uint32_t mask, void *data) {
    (void)mask; (void)data;
    char message[128] = {0};
    if (recv(fd, message, sizeof(message) - 1, 0) <= 0) return 0;
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    double x, y;
    unsigned int code, down;
    if (sscanf(message, "move %lf %lf", &x, &y) == 2) {
        notify_motion_absolute(&seat, &now, (struct weston_coord_global){.c = {x, y}});
        notify_pointer_frame(&seat);
    } else if (sscanf(message, "button %u %u", &code, &down) == 2) {
        notify_button(&seat, &now, code, down ? WL_POINTER_BUTTON_STATE_PRESSED : WL_POINTER_BUTTON_STATE_RELEASED);
        notify_pointer_frame(&seat);
    } else if (sscanf(message, "scroll %lf", &y) == 1) {
        /* Touchpad-style vertical scroll in surface pixels; negative scrolls up.
         * winit 0.30 turns only discrete steps from a wheel source into scrolling. */
        notify_axis_source(&seat, WL_POINTER_AXIS_SOURCE_FINGER);
        struct weston_pointer_axis_event axis = {.axis = WL_POINTER_AXIS_VERTICAL_SCROLL, .value = y};
        notify_axis(&seat, &now, &axis);
        notify_pointer_frame(&seat);
    } else if (sscanf(message, "key %u %u", &code, &down) == 2) {
        notify_key(&seat, &now, code, down ? WL_KEYBOARD_KEY_STATE_PRESSED : WL_KEYBOARD_KEY_STATE_RELEASED, STATE_UPDATE_AUTOMATIC);
    }
    return 0;
}

static void destroy_seat(struct wl_listener *listener, void *data) {
    (void)listener; (void)data;
    wl_list_remove(&cleanup.link);
    if (input_source) wl_event_source_remove(input_source);
    if (input_fd >= 0) close(input_fd);
    if (surface_logger) wl_protocol_logger_destroy(surface_logger);
    if (surface_log) fclose(surface_log);
    weston_seat_release(&seat);
}

WL_EXPORT int wet_module_init(struct weston_compositor *compositor, int *argc, char *argv[]) {
    (void)argc; (void)argv;
    weston_seat_init(&seat, compositor, "yapfast-private-virtual-seat");
    if (weston_seat_init_pointer(&seat) < 0 || weston_seat_init_keyboard(&seat, NULL) < 0)
        return -1;
    weston_compositor_add_destroy_listener_once(compositor, &cleanup, destroy_seat);
    const char *runtime = getenv("XDG_RUNTIME_DIR");
    if (!runtime || !getenv("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY")) return -1;
    const char *log_path = getenv("YAPFAST_FRAMEWORK_SURFACE_LOG");
    if (log_path && log_path[0]) {
        surface_log = fopen(log_path, "w");
        if (!surface_log) {
            weston_log("Could not open private surface update log: %s\n", strerror(errno));
            return -1;
        }
        /* Separate update-rate probes only. Flush on compositor shutdown. */
        setvbuf(surface_log, NULL, _IOFBF, 65536);
        surface_logger = wl_display_add_protocol_logger(compositor->wl_display,
            log_surface_commit, compositor);
        if (!surface_logger) {
            weston_log("Could not register private surface update logger\n");
            return -1;
        }
    }
    struct sockaddr_un address = {.sun_family = AF_UNIX};
    if (snprintf(address.sun_path, sizeof(address.sun_path), "%s/yapfast-input.sock", runtime) >= (int)sizeof(address.sun_path)) return -1;
    input_fd = socket(AF_UNIX, SOCK_DGRAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
    if (input_fd < 0 || bind(input_fd, (struct sockaddr *)&address, sizeof(address)) < 0) return -1;
    input_source = wl_event_loop_add_fd(wl_display_get_event_loop(compositor->wl_display), input_fd, WL_EVENT_READABLE, input_event, NULL);
    return 0;
}
