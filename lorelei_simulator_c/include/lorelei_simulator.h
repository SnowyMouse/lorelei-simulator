// SPDX-License-Identifier: GPL-3.0-only

#ifndef LORELEI_SIMULATOR_H
#define LORELEI_SIMULATOR_H

#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct Simulator Simulator;

/**
 * Create a new simulator
 *
 * @param rom              rom data
 * @param rom_size         rom size
 * @param save_state       save state data
 * @param save_state_size  save state size
 * @param number_of_trials max number of trials; if null, never end
 *
 * @returns the simulator, or null if an error occurred
 */
Simulator *simulator_new(
    const void *rom,
    size_t rom_size,
    const void *save_state,
    size_t save_state_size,
    const size_t *number_of_trials
);

/**
 * Free the simulator, stopping it if it is running.
 *
 * @param simulator simulator to check
 */
void simulator_free(
    Simulator *simulator
);

/**
 * Start the simulator. Crashes if it is already running.
 *
 * @param simulator    simulator to check
 * @param thread_count number of threads to use; if 0, automatically determine how many CPU threads you have
 */
void simulator_start(
    Simulator *simulator,
    size_t thread_count
);

/**
 * Stop the simulator if it's running.
 *
 * @param simulator simulator to check
 */
void simulator_stop(
    Simulator *simulator
);

/**
 * Check if the simulator is running.
 *
 * @param simulator simulator to check
 *
 * @returns true if the simulator is running
 */
bool simulator_is_running(
    const Simulator *simulator
);

/**
 * Get the current results for the simulation.
 *
 * @param simulator simulator to check
 * @param indices   pointer to move indices (must have at least size available)
 * @param counts    pointer to move counts (must have at least size available)
 * @param size      length of indices and counts; this will be overwritten with the size written
 */
void simulator_results(
    const Simulator *simulator,
    uint8_t *indices,
    uint64_t *counts,
    size_t *size
);

/**
 * Get the move name for the move with the index.
 *
 * @returns a null terminated C string if the index exists, or NULL if not
 */
const char *simulator_move_name(uint8_t index);

#endif
