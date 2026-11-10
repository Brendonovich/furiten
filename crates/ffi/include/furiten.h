// Furiten's core behind a C ABI. Requests and replies are the local core protocol's JSON lines (docs/contract.md).
#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FuritenCore FuritenCore;

// Called on a core thread after any write, including writes arriving through sync.
typedef void (*FuritenChanged)(void *ctx);

// Opens the core for `home` (created if missing). On failure returns NULL and, if `error` isn't NULL, sets it to a
// `{ code, message }` JSON string to free with furiten_string_free.
FuritenCore *furiten_open(const char *home, FuritenChanged on_change, void *ctx, char **error);

// Answers one request line (`{ id, method, params }`) with its reply line. Blocking; free with furiten_string_free.
char *furiten_request(FuritenCore *core, const char *request);

// Asks the core to sync now if signed in, as when the app comes back to the foreground.
void furiten_sync_soon(FuritenCore *core);

void furiten_string_free(char *s);

// Pushes what's left to sync, then closes the core.
void furiten_close(FuritenCore *core);

#ifdef __cplusplus
}
#endif
