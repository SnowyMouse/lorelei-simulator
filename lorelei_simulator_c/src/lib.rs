use std::ffi::c_char;
use std::num::NonZeroUsize;
use std::ptr::null;
use lorelei_simulator::Simulator;

#[no_mangle]
pub unsafe extern "C" fn simulator_new(
    rom: *const u8,
    rom_size: usize,
    save_state: *const u8,
    save_state_size: usize,
    number_of_trials: *const usize
) -> *mut Simulator {
    let rom = std::slice::from_raw_parts(rom, rom_size);
    let save_state = std::slice::from_raw_parts(save_state, save_state_size);
    let number_of_trials = if number_of_trials.is_null() {
        None
    }
    else {
        Some(*number_of_trials as u64)
    };
    match Simulator::new_from_slices(
        rom, save_state, number_of_trials
    ) {
        Ok(n) => Box::into_raw(Box::new(n)),
        Err(_) => std::ptr::null_mut()
    }
}

#[no_mangle]
pub unsafe extern "C" fn simulator_free(simulator: *mut Simulator) {
    if !simulator.is_null() {
        drop(Box::from_raw(simulator))
    }
}

#[no_mangle]
pub extern "C" fn simulator_start(simulator: &mut Simulator, thread_count: usize) {
    let threads = if thread_count == 0 {
        std::thread::available_parallelism().unwrap_or(NonZeroUsize::new(1).unwrap())
    }
    else {
        NonZeroUsize::new(thread_count).unwrap()
    };
    simulator.start(threads)
}

#[no_mangle]
pub extern "C" fn simulator_stop(simulator: &mut Simulator) {
    simulator.stop()
}

#[no_mangle]
pub extern "C" fn simulator_is_running(simulator: &Simulator) -> bool {
    simulator.is_running()
}

#[no_mangle]
pub unsafe extern "C" fn simulator_results(simulator: &Simulator, indices: *mut u8, counts: *mut u64, size: &mut usize) {
    let result = simulator.results();

    let mut indices = std::slice::from_raw_parts_mut(indices, *size).iter_mut();
    let mut counts = std::slice::from_raw_parts_mut(counts, *size).iter_mut();
    *size = result.len();

    for i in result {
        let (Some(index), Some(count)) = (indices.next(), counts.next()) else {
            return;
        };
        *index = i.0;
        *count = i.1;
    }
}


#[no_mangle]
pub extern "C" fn simulator_move_name(index: u8) -> *const c_char {
    const MOVES: [[u8; 16]; 256] = {
        use lorelei_simulator::move_name;

        let mut data = [[0u8; 16]; 256];
        let mut index = 1usize;

        while let Some(n) = move_name(index as u8) {
            let bytes = n.as_bytes();
            let mut char = 0usize;
            loop {
                if bytes.len() == char {
                    break;
                }
                data[index][char] = bytes[char];
                char += 1;
            }
            index += 1;
        }

        data
    };

    let data = &MOVES[index as usize];
    if data[0] == 0 {
        null()
    }
    else {
        data.as_ptr() as *const c_char
    }
}
