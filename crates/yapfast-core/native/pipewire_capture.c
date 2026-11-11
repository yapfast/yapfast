/* A screen on the GPU, on Linux: the DMA-BUF buffers of a PipeWire node the
 * desktop's screen sharing portal set up.
 *
 * The go-live path captures with libwebrtc's own capturer, which hands over I420
 * in memory: every frame is copied out of the compositor, converted, and
 * uploaded before an encoder sees it. native/wgc.cc and native/dup.cc took that
 * copy out of the Windows path, by handing the encoder the compositor's own
 * D3D11 texture. This is the same idea for Wayland, whose desktops share a
 * screen as a PipeWire stream of DRM objects: a buffer here is an fd referring
 * to a GEM object the compositor rendered into, and an encoder that imports that
 * fd on the same GPU reads the frame where it lies. Nothing in this file maps a
 * buffer, and nothing here allocates a byte per frame.
 *
 * Three things a caller has to know:
 *
 *   - the buffer is borrowed. `yapfast_pw_frame` hands out an fd that belongs to
 *     the stream, and it stays valid until the next `yapfast_pw_frame` or
 *     `yapfast_pw_close` on the same capture, whichever comes first; this file
 *     returns it to PipeWire then. A caller that has to hold a frame past the
 *     next call `dup`s the fd itself and closes that copy when it's done with it.
 *     Exactly one buffer is held at a time, so a stopped stream stops pinning a
 *     compositor allocation.
 *   - only DMA-BUFs are asked for and only theirs are accepted: the format pod
 *     carries `SPA_DATA_DmaBuf` and a modifier, and a producer that hands back
 *     memory mapped through the processor instead is refused by name rather than
 *     silently giving a caller pixels it will read wrongly.
 *   - the fd comes from the portal, not from the default PipeWire socket.
 *     `yapfast_pw_open` takes it already connected to the portal's session and
 *     takes ownership of it, since a sandboxed PipeWire client — which is what
 *     the portal makes for a screencast — can only see the exported node through
 *     that connection.
 *
 * Threads: the stream runs on a `pw_thread_loop` of its own, whose lock is also
 * the hand-off. `process` runs on that thread with its lock held and only
 * records the buffer and signals; `yapfast_pw_frame` takes the same lock, waits
 * on it with a timeout, and does the dequeue and requeue inside it, which is what
 * PipeWire asks for and the reason there is no mutex of ours here. */
/* PipeWire's headers read `locale_t` and don't include what declares it, so
 * build.rs puts <locale.h> in front of this file: see native/pipewire_capture.c's
 * entry there. */
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/types.h>
#include <time.h>

#include <dlfcn.h>

#include <pipewire/pipewire.h>
#include <spa/buffer/buffer.h>
#include <spa/param/buffers.h>
#include <spa/param/video/format-utils.h>
#include <spa/param/video/raw.h>
#include <spa/pod/builder.h>

/* PipeWire's client functions, bound here rather than by the link.
 *
 * LiveKit's libwebrtc carries a weak stub of each of them, for builds that have
 * no PipeWire (`pipewire_stubs.o`), and a stub is a trampoline through a pointer
 * that ends up naming the stub itself when the archive is scanned before
 * libpipewire-0.3 — so the call lands at address 0, which is what a screen share
 * did: a jump to 0 inside pthread_once's call to pw_init. Binding the functions
 * from the library that is loaded leaves the link's choice of definitions out of
 * it, and a machine without PipeWire gets a sentence instead of a crash. The
 * types are the headers' own, so a mismatch is a compile error here. */
typedef struct {
  typeof(pw_context_connect_fd) *context_connect_fd;
  typeof(pw_context_destroy) *context_destroy;
  typeof(pw_context_new) *context_new;
  typeof(pw_core_disconnect) *core_disconnect;
  typeof(pw_init) *init;
  typeof(pw_properties_new) *properties_new;
  typeof(pw_stream_add_listener) *stream_add_listener;
  typeof(pw_stream_connect) *stream_connect;
  typeof(pw_stream_dequeue_buffer) *stream_dequeue_buffer;
  typeof(pw_stream_destroy) *stream_destroy;
  typeof(pw_stream_new) *stream_new;
  typeof(pw_stream_queue_buffer) *stream_queue_buffer;
  typeof(pw_thread_loop_destroy) *thread_loop_destroy;
  typeof(pw_thread_loop_get_loop) *thread_loop_get_loop;
  typeof(pw_thread_loop_get_time) *thread_loop_get_time;
  typeof(pw_thread_loop_lock) *thread_loop_lock;
  typeof(pw_thread_loop_new) *thread_loop_new;
  typeof(pw_thread_loop_signal) *thread_loop_signal;
  typeof(pw_thread_loop_start) *thread_loop_start;
  typeof(pw_thread_loop_stop) *thread_loop_stop;
  typeof(pw_thread_loop_timed_wait_full) *thread_loop_timed_wait_full;
  typeof(pw_thread_loop_unlock) *thread_loop_unlock;
} yapfast_pw_api;

/* The definitions below are the library's, so every call is written as it was. */
static yapfast_pw_api api;
#define pw_context_connect_fd api.context_connect_fd
#define pw_context_destroy api.context_destroy
#define pw_context_new api.context_new
#define pw_core_disconnect api.core_disconnect
#define pw_init api.init
#define pw_properties_new api.properties_new
#define pw_stream_add_listener api.stream_add_listener
#define pw_stream_connect api.stream_connect
#define pw_stream_dequeue_buffer api.stream_dequeue_buffer
#define pw_stream_destroy api.stream_destroy
#define pw_stream_new api.stream_new
#define pw_stream_queue_buffer api.stream_queue_buffer
#define pw_thread_loop_destroy api.thread_loop_destroy
#define pw_thread_loop_get_loop api.thread_loop_get_loop
#define pw_thread_loop_get_time api.thread_loop_get_time
#define pw_thread_loop_lock api.thread_loop_lock
#define pw_thread_loop_new api.thread_loop_new
#define pw_thread_loop_signal api.thread_loop_signal
#define pw_thread_loop_start api.thread_loop_start
#define pw_thread_loop_stop api.thread_loop_stop
#define pw_thread_loop_timed_wait_full api.thread_loop_timed_wait_full
#define pw_thread_loop_unlock api.thread_loop_unlock

/* Fills the table from the library that is loaded. The portal's socket is what
 * reaches the shared node, not anything about which library this is, so any
 * libpipewire-0.3 will do; one that isn't there is said by name. */
static int bind_api(char *why, size_t why_size) {
  /* Set only once every entry point is in the table: a library missing one
   * fails this open and every one after it, rather than letting the next call
   * through to a NULL function. */
  static void *bound;
  if (bound != NULL) return 0;
  void *library = dlopen("libpipewire-0.3.so.0", RTLD_NOW | RTLD_LOCAL);
  if (library == NULL) {
    snprintf(why, why_size, "PipeWire's library isn't here (%s)", dlerror());
    return -1;
  }
#define YAPFAST_PW_BIND(field)                                                \
  *(void **)(&api.field) = dlsym(library, "pw_" #field);                      \
  if (api.field == NULL) {                                                    \
    snprintf(why, why_size, "libpipewire-0.3 has no pw_" #field);             \
    dlclose(library);                                                         \
    return -1;                                                                \
  }
  YAPFAST_PW_BIND(context_connect_fd)
  YAPFAST_PW_BIND(context_destroy)
  YAPFAST_PW_BIND(context_new)
  YAPFAST_PW_BIND(core_disconnect)
  YAPFAST_PW_BIND(init)
  YAPFAST_PW_BIND(properties_new)
  YAPFAST_PW_BIND(stream_add_listener)
  YAPFAST_PW_BIND(stream_connect)
  YAPFAST_PW_BIND(stream_dequeue_buffer)
  YAPFAST_PW_BIND(stream_destroy)
  YAPFAST_PW_BIND(stream_new)
  YAPFAST_PW_BIND(stream_queue_buffer)
  YAPFAST_PW_BIND(thread_loop_destroy)
  YAPFAST_PW_BIND(thread_loop_get_loop)
  YAPFAST_PW_BIND(thread_loop_get_time)
  YAPFAST_PW_BIND(thread_loop_lock)
  YAPFAST_PW_BIND(thread_loop_new)
  YAPFAST_PW_BIND(thread_loop_signal)
  YAPFAST_PW_BIND(thread_loop_start)
  YAPFAST_PW_BIND(thread_loop_stop)
  YAPFAST_PW_BIND(thread_loop_timed_wait_full)
  YAPFAST_PW_BIND(thread_loop_unlock)
#undef YAPFAST_PW_BIND
  bound = library;
  return 0;
}

/* The answers `yapfast_pw_frame` gives, numbered as native/wgc.cc numbers the
 * same four kinds of answer, since a caller switching on them reads both. */
#define YAPFAST_PW_CLOSED -1
#define YAPFAST_PW_FAILED -2
#define YAPFAST_PW_STILL 0
#define YAPFAST_PW_FRAME 1
#define YAPFAST_PW_RESIZED 2

/* One frame, as the caller reads it: an fd, and where the picture is in it. The
 * fields are the negotiated format's, so a caller importing the fd knows the
 * whole of what it is importing. */
struct yapfast_pw_frame {
  int fd;
  uint32_t offset;
  int32_t stride;
  uint32_t size;
  uint32_t format;
  uint64_t modifier;
  uint32_t width;
  uint32_t height;
};

struct yapfast_pw {
  struct pw_thread_loop *loop;
  struct pw_context *context;
  struct pw_core *core;
  struct pw_stream *stream;
  struct spa_hook stream_listener;
  struct spa_hook core_listener;
  uint32_t node_id;

  /* What was negotiated, and whether it has changed under a caller that has
   * already been told a size. */
  struct spa_video_info_raw raw;
  bool have_format;
  bool resized;

  /* The one buffer this holds: `pending` was dequeued and not yet given out,
   * `held` was given out and not yet taken back. Both are NULL when idle. */
  struct pw_buffer *pending;
  struct pw_buffer *held;

  bool closed;
  bool failed;
  char why[256];

  /* What the stream did, in the order it did it: the states it passed through and
   * what the producer answered an enumeration with. A stream that connects and
   * then negotiates nothing says so here rather than leaving `describe` to report
   * silence, since PipeWire reports a refused enumeration as a property with no
   * pod, which is neither an error nor a frame. */
  char trace[192];

  /* What `yapfast_pw_describe` says, and the counters behind it. */
  unsigned frames;
  unsigned stills;
  unsigned resizes;
};

static void pw_init_once(void) { pw_init(NULL, NULL); }

static pthread_once_t init_once = PTHREAD_ONCE_INIT;
static int init_status = 0;

static void set_why(struct yapfast_pw *pw, const char *text) {
  snprintf(pw->why, sizeof(pw->why), "%s", text);
}

/* Adds one thing the stream did to its trace, in the order it happened. */
static void note(struct yapfast_pw *pw, const char *pattern, ...) {
  size_t used = strlen(pw->trace);
  if (used + 2 >= sizeof(pw->trace)) return;
  if (used > 0) {
    pw->trace[used++] = ';';
    pw->trace[used++] = ' ';
    pw->trace[used] = '\0';
  }
  va_list arguments;
  va_start(arguments, pattern);
  vsnprintf(pw->trace + used, sizeof(pw->trace) - used, pattern, arguments);
  va_end(arguments);
}

/* A param's name, for the trace: the three a negotiated stream is made of, and
 * the rest by number. */
static const char *param_name(uint32_t id) {
  switch ((enum spa_param_type)id) {
    case SPA_PARAM_EnumFormat:
      return "EnumFormat";
    case SPA_PARAM_Format:
      return "Format";
    case SPA_PARAM_Buffers:
      return "Buffers";
    case SPA_PARAM_Latency:
      return "Latency";
    default:
      return "another param";
  }
}

/* A stream's state, for the trace. */
static const char *state_name(enum pw_stream_state state) {
  switch (state) {
    case PW_STREAM_STATE_ERROR:
      return "error";
    case PW_STREAM_STATE_UNCONNECTED:
      return "unconnected";
    case PW_STREAM_STATE_CONNECTING:
      return "connecting";
    case PW_STREAM_STATE_PAUSED:
      return "paused";
    case PW_STREAM_STATE_STREAMING:
      return "streaming";
    default:
      return "another state";
  }
}

/* Sizes as a person reads them. */
static const char *format_name(uint32_t format) {
  switch ((enum spa_video_format)format) {
    case SPA_VIDEO_FORMAT_BGRA:
      return "BGRA";
    case SPA_VIDEO_FORMAT_RGBA:
      return "RGBA";
    case SPA_VIDEO_FORMAT_BGRx:
      return "BGRx";
    case SPA_VIDEO_FORMAT_RGBx:
      return "RGBx";
    case SPA_VIDEO_FORMAT_RGB:
      return "RGB";
    default:
      return "a format this build doesn't name";
  }
}

/* One buffer, as `process` left it: borrowing the fd, the descriptor says where
 * the picture is. */
static int describe_buffer(struct yapfast_pw *pw, struct yapfast_pw_frame *frame) {
  struct pw_buffer *buffer = pw->pending;
  struct spa_buffer *spa_buffer = buffer->buffer;
  if (spa_buffer->n_datas < 1 || spa_buffer->datas[0].chunk == NULL) {
    set_why(pw, "the producer handed over a buffer with no plane in it");
    pw->failed = true;
    return YAPFAST_PW_FAILED;
  }
  struct spa_data *data = &spa_buffer->datas[0];
  if (data->type != SPA_DATA_DmaBuf) {
    snprintf(pw->why, sizeof(pw->why),
             "the producer negotiated a %s buffer, not a DMA-BUF, so its frames would have to "
             "be copied through the processor",
             data->type == SPA_DATA_MemFd   ? "MemFd"
             : data->type == SPA_DATA_MemPtr ? "MemPtr"
                                             : "non-DMA-BUF");
    pw->failed = true;
    return YAPFAST_PW_FAILED;
  }
  if (!pw->have_format) {
    set_why(pw, "the producer sent a buffer before it said what format it is");
    pw->failed = true;
    return YAPFAST_PW_FAILED;
  }
  frame->fd = data->fd;
  /* A plane can sit anywhere in the allocation: `mapoffset` is where the mapped
   * region starts and the chunk's offset is where the picture starts in that. */
  frame->offset = data->mapoffset + data->chunk->offset;
  frame->stride = data->chunk->stride;
  frame->size = data->chunk->size;
  frame->format = (uint32_t)pw->raw.format;
  frame->modifier = pw->raw.modifier;
  frame->width = (uint32_t)pw->raw.size.width;
  frame->height = (uint32_t)pw->raw.size.height;
  return YAPFAST_PW_FRAME;
}

/* Returns the buffer a previous call gave out, so the producer can fill it
 * again. Called with the loop lock held. */
static void let_go(struct yapfast_pw *pw) {
  if (pw->held != NULL) {
    pw_stream_queue_buffer(pw->stream, pw->held);
    pw->held = NULL;
  }
}

static void on_process(void *data) {
  struct yapfast_pw *pw = data;
  struct pw_buffer *buffer = pw_stream_dequeue_buffer(pw->stream);
  if (buffer == NULL) return;
  /* Nobody has taken the last one; keep the newest, since a frame the caller
   * skipped is a frame it would have dropped anyway. */
  if (pw->pending != NULL) pw_stream_queue_buffer(pw->stream, pw->pending);
  pw->pending = buffer;
  pw->frames++;
  pw_thread_loop_signal(pw->loop, false);
}

static void on_param_changed(void *data, uint32_t id, const struct spa_pod *param) {
  struct yapfast_pw *pw = data;
  if (param == NULL) {
    /* The producer answered an enumeration with nothing, which is how PipeWire
     * says that none of the properties offered could be made. Nothing is a frame
     * and nothing is an error, so without this the capture would sit at "no format
     * negotiated yet" with no reason anywhere. */
    note(pw, "%s refused", param_name(id));
    return;
  }
  if (id != SPA_PARAM_Format) {
    note(pw, "%s", param_name(id));
    return;
  }
  struct spa_video_info info;
  if (spa_format_parse(param, &info.media_type, &info.media_subtype) < 0) return;
  if (info.media_type != SPA_MEDIA_TYPE_video || info.media_subtype != SPA_MEDIA_SUBTYPE_raw) return;
  if (spa_format_video_raw_parse(param, &info.info.raw) < 0) return;
  struct spa_video_info_raw raw = info.info.raw;
  note(pw, "Format %dx%d %s modifier 0x%llx", (int)raw.size.width, (int)raw.size.height,
       format_name((uint32_t)raw.format), (unsigned long long)raw.modifier);
  if (pw->have_format && (raw.format != pw->raw.format || raw.size.width != pw->raw.size.width ||
                          raw.size.height != pw->raw.size.height ||
                          raw.modifier != pw->raw.modifier)) {
    /* An encoder built for the old size reads the next frame wrongly, so the
     * caller is told before it is handed one. */
    pw->resized = true;
    pw->resizes++;
  }
  pw->raw = raw;
  pw->have_format = true;
  pw_thread_loop_signal(pw->loop, false);
}

static void on_state_changed(void *data, enum pw_stream_state old, enum pw_stream_state state,
                             const char *error) {
  struct yapfast_pw *pw = data;
  note(pw, "%s", state_name(state));
  switch (state) {
    case PW_STREAM_STATE_ERROR:
      snprintf(pw->why, sizeof(pw->why), "%s", error != NULL ? error : "the stream failed");
      pw->failed = true;
      pw->closed = true;
      break;
    case PW_STREAM_STATE_UNCONNECTED:
      /* The node the portal exported went away, which is how a screencast ends
       * when the desktop closes its session: reported as an end, not a failure. */
      if (old != PW_STREAM_STATE_UNCONNECTED) {
        set_why(pw, "the desktop closed the screen it was sharing");
        pw->closed = true;
      }
      break;
    default:
      break;
  }
  pw_thread_loop_signal(pw->loop, false);
}

static void on_core_error(void *data, uint32_t id, int seq, int res, const char *message) {
  struct yapfast_pw *pw = data;
  /* Only the connection going away is an end; `seq` and `res` are the request
   * that failed and how, which the message says in words. */
  (void)seq;
  (void)res;
  if (id != PW_ID_CORE) return;
  snprintf(pw->why, sizeof(pw->why), "%s", message != NULL ? message : "the PipeWire connection failed");
  pw->failed = true;
  pw->closed = true;
  pw_thread_loop_signal(pw->loop, false);
}

static const struct pw_stream_events stream_events = {
    PW_VERSION_STREAM_EVENTS,
    .state_changed = on_state_changed,
    .param_changed = on_param_changed,
    .process = on_process,
};

static const struct pw_core_events core_events = {
    PW_VERSION_CORE_EVENTS,
    .error = on_core_error,
};

/* Not `static`: src/capture/pipewire.rs calls it, and a static declaration here
 * would make the definition local too, leaving the linker without the symbol the
 * capture's drop needs. */
void yapfast_pw_close(struct yapfast_pw *pw);

/* Asks the connection for the one format this file can hand over: video, raw,
 * BGRA or RGBA as the producer likes, in a buffer with a modifier — which is
 * what says the frames are DMA-BUFs — and nothing else. The pod is the id
 * `pw_stream_connect` enumerates with, `SPA_PARAM_EnumFormat`, and not
 * `SPA_PARAM_Format`, which is the id a negotiated format comes back under in
 * `param_changed`: a pod built with the second is one the producer matches
 * against nothing and answers with silence.
 *
 * Two enumerations are offered, in this order. The first asks for a linear
 * modifier, the one an importer here maps most cheaply. The second names no
 * modifier at all, for a producer that can't make a linear buffer: a compositor
 * on a card with a tiling of its own answers "linear only" with nothing at all
 * — KWin on an NVIDIA screen hands over 0x0300000000e08014, block-linear — and a
 * share that negotiates nothing is a share that shows nothing, while a share on a
 * tiled picture is one the importer and the encoder are asked about with the
 * modifier in hand. Which of the two answered is in the Format the producer
 * sends, and the capture reports it.
 *
 * The last pod asks for the
 * planes of the one data type an encoder can import, so a producer that can't
 * give a DMA-BUF says so instead of quietly handing over memory the processor
 * would have to copy. */
static int build_params(struct spa_pod_builder *builder, struct spa_pod_frame *frames,
                        const struct spa_pod **params) {
  spa_pod_builder_push_object(builder, &frames[0], SPA_TYPE_OBJECT_Format, SPA_PARAM_EnumFormat);
  spa_pod_builder_add(builder, SPA_FORMAT_mediaType, SPA_POD_Id(SPA_MEDIA_TYPE_video),
                      SPA_FORMAT_mediaSubtype, SPA_POD_Id(SPA_MEDIA_SUBTYPE_raw),
                      SPA_FORMAT_VIDEO_format,
                      SPA_POD_CHOICE_ENUM_Id(2, SPA_VIDEO_FORMAT_BGRA, SPA_VIDEO_FORMAT_RGBA),
                      SPA_FORMAT_VIDEO_modifier,
                      /* Linear, and only linear — the modifier a reader here can map.
                       * The same value is named twice because the header PipeWire 1.6
                       * ships wants an alternative beside a choice's default, and a
                       * list with one value has none to give; 1.4's header took it
                       * either way, so this spelling is the one both compile. */
                      SPA_POD_CHOICE_ENUM_Long(2, (int64_t)0 /* DRM_FORMAT_MOD_LINEAR */,
                                               (int64_t)0 /* the same, again */),
                      0 /* the end of spa_pod_builder_add's arguments */);
  params[0] = spa_pod_builder_pop(builder, &frames[0]);

  /* The same picture again with no modifier named, for a producer that can't make
   * a linear buffer: it hands over its own tiling, which the Format it answers
   * with names, and the importer is asked about that. */
  spa_pod_builder_push_object(builder, &frames[1], SPA_TYPE_OBJECT_Format, SPA_PARAM_EnumFormat);
  spa_pod_builder_add(builder, SPA_FORMAT_mediaType, SPA_POD_Id(SPA_MEDIA_TYPE_video),
                      SPA_FORMAT_mediaSubtype, SPA_POD_Id(SPA_MEDIA_SUBTYPE_raw),
                      SPA_FORMAT_VIDEO_format,
                      SPA_POD_CHOICE_ENUM_Id(2, SPA_VIDEO_FORMAT_BGRA, SPA_VIDEO_FORMAT_RGBA),
                      0 /* the end of spa_pod_builder_add's arguments */);
  params[1] = spa_pod_builder_pop(builder, &frames[1]);

  spa_pod_builder_push_object(builder, &frames[2], SPA_TYPE_OBJECT_ParamBuffers, SPA_PARAM_Buffers);
  spa_pod_builder_add(builder, SPA_PARAM_BUFFERS_dataType, SPA_POD_Int((1 << SPA_DATA_DmaBuf)), 0);
  params[2] = spa_pod_builder_pop(builder, &frames[2]);
  return 0;
}

void *yapfast_pw_open(uint32_t node_id, int fd, char *why, size_t why_size) {
  if (why != NULL && why_size > 0) why[0] = '\0';
  if (bind_api(why, why_size) < 0) return NULL;
  pthread_once(&init_once, pw_init_once);
  (void)init_status;

  struct yapfast_pw *pw = calloc(1, sizeof(*pw));
  if (pw == NULL) {
    snprintf(why, why_size, "out of memory");
    return NULL;
  }
  pw->node_id = node_id;

  pw->loop = pw_thread_loop_new("yapfast-screen", NULL);
  if (pw->loop == NULL) {
    snprintf(why, why_size, "PipeWire's thread loop couldn't be made");
    free(pw);
    return NULL;
  }
  pw_thread_loop_lock(pw->loop);
  pw->context = pw_context_new(pw_thread_loop_get_loop(pw->loop), NULL, 0);
  if (pw->context == NULL) {
    pw_thread_loop_unlock(pw->loop);
    snprintf(why, why_size, "PipeWire's context couldn't be made");
    yapfast_pw_close(pw);
    return NULL;
  }
  /* The fd is the portal's, and pw_context_connect_fd takes it. */
  pw->core = pw_context_connect_fd(pw->context, fd, NULL, 0);
  if (pw->core == NULL) {
    pw_thread_loop_unlock(pw->loop);
    snprintf(why, why_size, "the portal's PipeWire connection couldn't be joined");
    yapfast_pw_close(pw);
    return NULL;
  }
  pw_core_add_listener(pw->core, &pw->core_listener, &core_events, pw);

  struct pw_properties *properties =
      pw_properties_new(PW_KEY_MEDIA_TYPE, "Video", PW_KEY_MEDIA_CATEGORY, "Capture",
                        PW_KEY_MEDIA_ROLE, "Screen", PW_KEY_NODE_NAME, "yapfast-screen", NULL);
  pw->stream = pw_stream_new(pw->core, "Yapfast screen", properties);
  if (pw->stream == NULL) {
    pw_thread_loop_unlock(pw->loop);
    snprintf(why, why_size, "PipeWire's stream couldn't be made");
    yapfast_pw_close(pw);
    return NULL;
  }
  pw_stream_add_listener(pw->stream, &pw->stream_listener, &stream_events, pw);

  uint8_t pod_buffer[2048];
  struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(pod_buffer, sizeof(pod_buffer));
  struct spa_pod_frame frames[3];
  const struct spa_pod *params[3];
  build_params(&builder, frames, params);
  int res = pw_stream_connect(pw->stream, PW_DIRECTION_INPUT, node_id,
                              PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_DONT_RECONNECT, params,
                              3);
  if (res < 0) {
    pw_thread_loop_unlock(pw->loop);
    snprintf(why, why_size, "the screen's stream couldn't be connected: %s", strerror(-res));
    yapfast_pw_close(pw);
    return NULL;
  }
  res = pw_thread_loop_start(pw->loop);
  pw_thread_loop_unlock(pw->loop);
  if (res < 0) {
    snprintf(why, why_size, "PipeWire's thread couldn't be started: %s", strerror(-res));
    yapfast_pw_close(pw);
    return NULL;
  }
  return pw;
}

int yapfast_pw_frame(struct yapfast_pw *pw, struct yapfast_pw_frame *frame, int timeout_ms,
                     char *why, size_t why_size) {
  if (why != NULL && why_size > 0) why[0] = '\0';
  if (pw->failed) {
    snprintf(why, why_size, "%s", pw->why);
    return YAPFAST_PW_FAILED;
  }
  if (pw->closed) return YAPFAST_PW_CLOSED;

  pw_thread_loop_lock(pw->loop);
  /* A size that changed under the caller is told before a frame of it is handed
   * over, so an encoder rebuilt on the news reads the frame that comes after. */
  if (pw->resized) {
    pw->resized = false;
    pw_thread_loop_unlock(pw->loop);
    return YAPFAST_PW_RESIZED;
  }
  if (pw->pending == NULL) {
    struct timespec deadline;
    pw_thread_loop_get_time(pw->loop, &deadline, (int64_t)timeout_ms * SPA_NSEC_PER_MSEC);
    /* 0 when another thread signalled the wait and a negative errno when the
     * deadline came first, which for a full wait is only ETIMEDOUT: a screen that
     * hasn't changed, and not a failure. */
    int res = pw_thread_loop_timed_wait_full(pw->loop, &deadline);
    if (res != 0) {
      pw->stills++;
      int answer = pw->failed ? YAPFAST_PW_FAILED : pw->closed ? YAPFAST_PW_CLOSED : YAPFAST_PW_STILL;
      if (answer == YAPFAST_PW_FAILED) snprintf(why, why_size, "%s", pw->why);
      pw_thread_loop_unlock(pw->loop);
      return answer;
    }
    if (pw->failed) {
      snprintf(why, why_size, "%s", pw->why);
      pw_thread_loop_unlock(pw->loop);
      return YAPFAST_PW_FAILED;
    }
    if (pw->closed) {
      pw_thread_loop_unlock(pw->loop);
      return YAPFAST_PW_CLOSED;
    }
    if (pw->resized) {
      pw->resized = false;
      pw_thread_loop_unlock(pw->loop);
      return YAPFAST_PW_RESIZED;
    }
    if (pw->pending == NULL) {
      /* Woken for the format alone: say a frame hasn't come rather than hand
       * over nothing. */
      pw->stills++;
      pw_thread_loop_unlock(pw->loop);
      return YAPFAST_PW_STILL;
    }
  }
  let_go(pw);
  int answer = describe_buffer(pw, frame);
  if (answer == YAPFAST_PW_FRAME) {
    pw->held = pw->pending;
    pw->pending = NULL;
  } else if (answer == YAPFAST_PW_FAILED) {
    snprintf(why, why_size, "%s", pw->why);
  }
  pw_thread_loop_unlock(pw->loop);
  return answer;
}

void yapfast_pw_describe(struct yapfast_pw *pw, char *text, size_t text_size) {
  if (text == NULL || text_size == 0) return;
  if (!pw->have_format) {
    snprintf(text, text_size, "node %u, no format negotiated yet, %u frames, %s", pw->node_id,
             pw->frames, pw->trace[0] ? pw->trace : "nothing said yet");
    return;
  }
  char modifier[24];
  if (pw->raw.modifier == 0) {
    snprintf(modifier, sizeof(modifier), "linear");
  } else {
    snprintf(modifier, sizeof(modifier), "0x%016llx", (unsigned long long)pw->raw.modifier);
  }
  snprintf(text, text_size,
           "node %u, %ux%u %s DMA-BUF, modifier %s, %u frames, %u still, %u resizes", pw->node_id,
           (unsigned)pw->raw.size.width, (unsigned)pw->raw.size.height,
           format_name((uint32_t)pw->raw.format), modifier, pw->frames, pw->stills, pw->resizes);
}

void yapfast_pw_close(struct yapfast_pw *pw) {
  if (pw == NULL) return;
  if (pw->loop != NULL) {
    /* Stop the thread that would call the callbacks before anything it reads
     * goes away. */
    pw_thread_loop_stop(pw->loop);
    pw_thread_loop_lock(pw->loop);
    pw->closed = true;
    if (pw->pending != NULL) pw_stream_queue_buffer(pw->stream, pw->pending);
    pw->pending = NULL;
    pw->held = NULL;
    if (pw->stream != NULL) {
      spa_hook_remove(&pw->stream_listener);
      pw_stream_destroy(pw->stream);
      pw->stream = NULL;
    }
    if (pw->core != NULL) {
      spa_hook_remove(&pw->core_listener);
      pw_core_disconnect(pw->core);
      pw->core = NULL;
    }
    if (pw->context != NULL) {
      pw_context_destroy(pw->context);
      pw->context = NULL;
    }
    pw_thread_loop_unlock(pw->loop);
    pw_thread_loop_destroy(pw->loop);
    pw->loop = NULL;
  }
  free(pw);
}
