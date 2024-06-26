// SPDX-License-Identifier: GPL-3.0-only

#ifndef LORELEI_SIMULATOR_H
#define LORELEI_SIMULATOR_H

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct Simulator Simulator;

Simulator *simulator_new(
    const void *rom,
    size_t rom_size,
    const void *save_state,
    size_t save_state_size,
    const size_t *number_of_trials
);

void simulator_free(
    Simulator *simulator
);

void simulator_start(
    Simulator *simulator,
    size_t thread_count
);

void simulator_stop(
    Simulator *simulator
);

bool simulator_is_running(
    const Simulator *simulator
);

void simulator_results(
    const Simulator *simulator,
    uint8_t *indices,
    uint64_t *counts,
    size_t *size
);

#endif
