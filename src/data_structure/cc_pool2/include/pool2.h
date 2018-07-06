#pragma once

#include <stdint.h>
#include <cc_debug.h>

struct PoolHandle;

typedef void (*pool2_init_callback_t)(void *buf);

struct PoolHandle *
pool2_create_handle(
    size_t obj_size,
    uint32_t nmax,
    pool2_init_callback_t
);

void
pool2_destroy_handle(struct PoolHandle *handle_p);

void *
pool2_take(struct PoolHandle *handle_p);

void
pool2_put(struct PoolHandle *handle_p, void *buf);
