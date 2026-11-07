// Recall's core behind a C ABI. Requests and replies are the local core protocol's JSON lines (docs/contract.md).
#pragma once

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RecallCore RecallCore;

// Called on a core thread after any write, including writes arriving through sync.
typedef void (*RecallChanged)(void *ctx);

// Opens the core for `home` (created if missing). On failure returns NULL and, if `error` isn't NULL, sets it to a
// `{ code, message }` JSON string to free with recall_string_free.
RecallCore *recall_open(const char *home, RecallChanged on_change, void *ctx, char **error);

// Answers one request line (`{ id, method, params }`) with its reply line. Blocking; free with recall_string_free.
char *recall_request(RecallCore *core, const char *request);

// Asks the core to sync now if signed in, as when the app comes back to the foreground.
void recall_sync_soon(RecallCore *core);

void recall_string_free(char *s);

// Pushes what's left to sync, then closes the core.
void recall_close(RecallCore *core);

#ifdef __cplusplus
}
#endif
