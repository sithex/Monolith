// LD_PRELOAD shim: lets WebKitGTK (GTK3) find the real DRM CRTC for a monitor so
// requestAnimationFrame/compositing run at the panel's real refresh rate (e.g. 240 Hz)
// instead of the 60 Hz fallback timer.
//
// Why: WebKitGTK 2.54's GTK3 build matches a GdkMonitor to a DRM connector by
// physical size only (gdk_monitor_get_width_mm == connector->mmWidth). KWin/XWayland
// report the EDID detailed-timing size (e.g. 587x330 mm) while the kernel reports the
// EDID base-block size (590x330 mm), so nothing matches and WebKit logs
// "no drm node with CRTC found" and falls back to DisplayVBlankMonitorTimer (60 fps).
//
// Fix: after libdrm returns a connector, copy the physical size GDK reports for the
// monitor with the SAME connector name (e.g. "DP-1"). Nothing else is changed; if GDK
// is not initialised or no name matches, the connector is returned untouched.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct {
    uint32_t connector_id, encoder_id, connector_type, connector_type_id;
    int connection;
    uint32_t mmWidth, mmHeight;
    /* remaining drmModeConnector fields are not touched */
} ConnectorHead;

typedef void *(*get_connector_fn)(int, uint32_t);
typedef const char *(*type_name_fn)(uint32_t);

void *drmModeGetConnector(int fd, uint32_t id) {
    static get_connector_fn real;
    if (!real) real = (get_connector_fn)dlsym(RTLD_NEXT, "drmModeGetConnector");
    if (!real) return NULL;
    ConnectorHead *c = real(fd, id);
    if (!c || c->connection != 1) return c;

    type_name_fn type_name = (type_name_fn)dlsym(RTLD_DEFAULT, "drmModeGetConnectorTypeName");
    void *(*display_default)(void) = dlsym(RTLD_DEFAULT, "gdk_display_get_default");
    void *(*default_screen)(void *) = dlsym(RTLD_DEFAULT, "gdk_display_get_default_screen");
    int (*n_monitors)(void *) = dlsym(RTLD_DEFAULT, "gdk_display_get_n_monitors");
    void *(*get_monitor)(void *, int) = dlsym(RTLD_DEFAULT, "gdk_display_get_monitor");
    char *(*plug_name)(void *, int) = dlsym(RTLD_DEFAULT, "gdk_screen_get_monitor_plug_name");
    int (*width_mm)(void *) = dlsym(RTLD_DEFAULT, "gdk_monitor_get_width_mm");
    int (*height_mm)(void *) = dlsym(RTLD_DEFAULT, "gdk_monitor_get_height_mm");
    void (*gfree)(void *) = dlsym(RTLD_DEFAULT, "g_free");
    if (!type_name || !display_default || !default_screen || !n_monitors || !get_monitor ||
        !plug_name || !width_mm || !height_mm || !gfree)
        return c;

    const char *type = type_name(c->connector_type);
    void *display = display_default();
    if (!type || !display) return c;
    void *screen = default_screen(display);
    if (!screen) return c;

    char want[64];
    snprintf(want, sizeof want, "%s-%u", type, c->connector_type_id);
    int count = n_monitors(display);
    for (int i = 0; i < count; i++) {
        char *name = plug_name(screen, i);
        int match = name && strcmp(name, want) == 0;
        if (name) gfree(name);
        if (!match) continue;
        void *monitor = get_monitor(display, i);
        if (!monitor) break;
        int w = width_mm(monitor), h = height_mm(monitor);
        if (w > 0 && h > 0) { c->mmWidth = (uint32_t)w; c->mmHeight = (uint32_t)h; }
        break;
    }
    return c;
}
