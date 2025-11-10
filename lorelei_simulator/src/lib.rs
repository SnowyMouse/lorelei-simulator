use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle};
use rand::random;
use safeboy::*;

mod data;

#[derive(Copy, Clone, PartialEq, Debug)]
enum Game {
    Yellow,
    Red,
    Blue,

    Gold,
    Silver,
    Crystal
}

impl Display for Game {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Gold => "Pokémon: Gold Version",
            Self::Silver => "Pokémon: Silver Version",
            Self::Crystal => "Pokémon: Crystal Version",
            Self::Yellow => "Pokémon Yellow Version: Special Pikachu Edition",
            Self::Red => "Pokémon: Red Version",
            Self::Blue => "Pokémon: Blue Version",
        };
        f.write_str(s)
    }
}

pub struct Simulator {
    inner: Arc<SimulatorInner>,
    threads: Vec<JoinHandle<()>>
}
impl Simulator {
    pub fn new_from_slices(
        rom: &[u8],
        save_state: &[u8],
        trials: Option<u64>
    ) -> Result<Self, SimulatorError> {
        Self::new_from_vec(rom.to_vec(), save_state.to_vec(), trials)
    }

    pub fn new_from_vec(
        rom: Vec<u8>,
        save_state: Vec<u8>,
        trials: Option<u64>
    ) -> Result<Self, SimulatorError> {
        let Ok(model) = model_for_save_state(&save_state) else {
            return Err(SimulatorError::SaveStateError);
        };

        let mut gameboy = Gameboy::new(model);
        gameboy.load_rom(&rom);

        if gameboy.load_save_state(&save_state).is_err() {
            return Err(SimulatorError::SaveStateError);
        }

        let title = gameboy.get_rom_title();
        let game = match title {
            "POKEMON YELLOW" => Game::Yellow,
            "POKEMON RED" => Game::Red,
            "POKEMON BLUE" => Game::Blue,
            "POKEMON_GLDAAUE" => Game::Gold,
            "POKEMON_SLVAAXE" => Game::Silver,
            "PM_CRYSTAL" => Game::Crystal,
            n => {
                return Err(SimulatorError::UnknownGame {
                    name_len: n.len(),
                    game: {
                        let mut data = [0u8; 64];
                        data[..n.len()].copy_from_slice(n.as_bytes());
                        data
                    }
                })
            }
        };

        Ok(Self {
            inner: Arc::new(SimulatorInner {
                model,
                rom,
                save_state: Mutex::new(Arc::new(save_state)),
                sample_count: AtomicU64::new(0),
                trials,
                results: Mutex::new(Default::default()),
                stop: AtomicBool::new(false),
                running_threads: AtomicUsize::new(0),
                game,
            }),
            threads: Vec::new()
        })
    }

    pub fn is_running(&self) -> bool {
        self.inner.running_threads.load(Ordering::Relaxed) > 0
    }

    /// Get current results.
    pub fn results(&self) -> HashMap<u8, u64> {
        self.inner.results.lock().unwrap().clone()
    }

    /// Run the simulator with the given thread count.
    pub fn start(&mut self, thread_count: NonZeroUsize) {
        assert!(!self.is_running(), "already running");
        self.inner.stop.swap(false, Ordering::Relaxed);
        for _ in 0..thread_count.get() {
            let inner_cloned = self.inner.clone();
            self.inner.running_threads.fetch_add(1, Ordering::Relaxed);
            self.threads.push(std::thread::spawn(move || {
                simulate(inner_cloned.clone());
                inner_cloned.running_threads.fetch_sub(1, Ordering::Relaxed);
            }))
        }
    }

    pub fn stop(&mut self) {
        if !self.is_running() {
            return;
        }
        self.inner.stop.swap(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[derive(Copy, Clone)]
pub enum SimulatorError {
    SaveStateError,
    UnknownGame { game: [u8; 64], name_len: usize }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Display for SimulatorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SimulatorError::SaveStateError => f.write_str("Can't read save state"),
            SimulatorError::UnknownGame { game, name_len } => {
                let game_name = std::str::from_utf8(&game[..*name_len]).unwrap();
                f.write_fmt(format_args!("Unknown game {game_name} from ROM"))
            }
        }
    }
}

struct SimulatorInner {
    model: Model,
    rom: Vec<u8>,
    save_state: Mutex<Arc<Vec<u8>>>,
    sample_count: AtomicU64,
    trials: Option<u64>,
    results: Mutex<HashMap<u8, u64>>,
    running_threads: AtomicUsize,
    stop: AtomicBool,
    game: Game
}

struct Status {
    rng_hit: Rc<AtomicBool>,
    decision_made: Rc<AtomicU8>,
    game: Game
}

impl GameboyCallbacks for Status {
    fn read_memory(&mut self, _instance: &mut RunningGameboy, address: u16, original_data: u8) -> u8 {
        match self.game {
            Game::Red | Game::Blue | Game::Yellow => {
                if address == 0xFFD3 || address == 0xFFD4 {
                    self.rng_hit.swap(true, Ordering::Relaxed);
                    return random();
                }
                original_data
            }
            Game::Gold | Game::Silver => {
                if address == 0xFFE3 || address == 0xFFE4 {
                    self.rng_hit.swap(true, Ordering::Relaxed);
                    return random();
                }
                original_data
            },
            Game::Crystal => {
                if address == 0xFFE1 || address == 0xFFE2 {
                    self.rng_hit.swap(true, Ordering::Relaxed);
                    return random();
                }
                original_data
            },
        }
    }

    fn write_memory(&mut self, instance: &mut RunningGameboy, address: u16, data: u8) -> bool {
        match self.game {
            Game::Red | Game::Blue | Game::Yellow => {
                if address == 0xCCDD && data != 0 {
                    self.decision_made.swap(data, Ordering::Relaxed);
                }
                true
            }
            Game::Gold | Game::Silver | Game::Crystal => {
                let (enemy_current_move_addr, enemy_current_move_num_addr) = if self.game == Game::Crystal {
                    (0xC6E4, 0xC6E9)
                }
                else {
                    (0xCBC2, 0xCBC7)
                };

                if address == enemy_current_move_addr && data != 0 {
                    let pc = instance.get_registers().pc as usize;
                    if pc > 0x4000 {
                        let offset = pc - 0x4000;
                        let DirectAccessData { data: rom, bank } = instance.direct_access(DirectAccessRegion::ROM);
                        let rom = &rom[0x4000 * bank as usize..];
                        let rom = rom.get(offset..offset + 6);
                        let high = (enemy_current_move_num_addr >> 8) as u8;
                        let low = (enemy_current_move_num_addr & 0xFF) as u8;

                        // use a signature so ROM hacks can work provided RAM isn't moved around too much
                        if rom == Some(&[0x79, 0xEA, low, high, 0xC9, 0x91]) {
                            self.decision_made.swap(data, Ordering::Relaxed);
                        }
                    }
                }

                true
            }
        }
    }
}

fn simulate(inner: Arc<SimulatorInner>) {
    let mut gameboy = Gameboy::new(inner.model);
    gameboy.load_rom(inner.rom.as_slice());
    gameboy.set_turbo_mode(TurboMode::Enabled);
    gameboy.set_memory_callbacks_enabled(true);

    let mut save_state = Arc::clone(&inner.save_state.lock().unwrap());
    let mut found_best_save_state = false;

    loop {
        // We can load to the first instance of the random number generator if possible.
        gameboy.load_save_state(&save_state).unwrap();

        let rng_hit = Rc::new(AtomicBool::new(false));
        let decision_made = Rc::new(AtomicU8::new(0));

        let memes = Status {
            rng_hit: rng_hit.clone(),
            decision_made: decision_made.clone(),
            game: inner.game
        };
        gameboy.set_callbacks(Some(Box::new(memes)));

        let mut rapid_fire = 0u8;
        let mut odd_frame = false;

        let move_found = loop {
            if inner.stop.load(Ordering::Relaxed) {
                return;
            }

            if !found_best_save_state {
                if rng_hit.load(Ordering::Relaxed) {
                    // We found where the first random() call is!
                    //
                    // Cache this for further calls to simulate().
                    *inner.save_state.lock().unwrap() = save_state.clone();
                    found_best_save_state = true;
                }
                else {
                    save_state = Arc::new(gameboy.create_save_state());
                }
            }

            if odd_frame != gameboy.is_odd_frame() {
                rapid_fire = (rapid_fire + 1) % 6;
                gameboy.set_input_button_state(InputButton::A, rapid_fire < 3);
                odd_frame = !odd_frame;
            }

            let result = decision_made.load(Ordering::Relaxed);
            if result != 0 {
                break result;
            }

            gameboy.run();
        };

        let new_count = inner.sample_count.fetch_add(1, Ordering::Relaxed);
        if inner.trials.is_some_and(|t| new_count >= t) {
            inner.sample_count.fetch_sub(1, Ordering::Relaxed);
            return;
        }

        let mut hm = inner.results.lock().unwrap();
        if let Some(n) = hm.get_mut(&move_found) {
            *n += 1;
        }
        else {
            hm.insert(move_found, 1);
        }
    }
}

pub const fn move_name(move_index: u8) -> Option<&'static str> {
    match data::MoveType::from_u8(move_index) {
        Some(n) => Some(n.name()),
        None => None
    }
}
