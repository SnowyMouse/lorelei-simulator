use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle};
use rand::random;
use safeboy::types::{DirectAccess, Key, Model};

#[derive(Copy, Clone)]
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
        let Ok(model) = safeboy::Gameboy::model_for_save_state(&save_state) else {
            return Err(SimulatorError::SaveStateError);
        };

        let mut gameboy = safeboy::Gameboy::new(model);
        gameboy.load_rom_from_buffer(&rom);

        if gameboy.load_state_from_buffer(&save_state).is_err() {
            return Err(SimulatorError::SaveStateError);
        }

        let title = gameboy.get_rom_title();
        let game = match title.as_str() {
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
                save_state,
                sample_count: AtomicU64::new(0),
                trials,
                results: Mutex::new(Default::default()),
                should_be_running: AtomicBool::new(false),
                game,
            }),
            threads: Vec::new()
        })
    }

    pub fn is_running(&self) -> bool {
        self.inner.should_be_running.load(Ordering::Relaxed)
    }

    pub fn results(&self) -> HashMap<u8, u64> {
        self.inner.results.lock().unwrap().clone()
    }

    pub fn start(&mut self, thread_count: NonZeroUsize) {
        assert!(!self.is_running(), "already running");
        for _ in 0..thread_count.get() {
            let inner_cloned = self.inner.clone();
            self.threads.push(std::thread::spawn(|| simulate(inner_cloned)))
        }
    }

    pub fn stop(&mut self) {
        if !self.is_running() {
            return;
        }
        self.inner.should_be_running.swap(false, Ordering::Relaxed);
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
    save_state: Vec<u8>,
    sample_count: AtomicU64,
    trials: Option<u64>,
    results: Mutex<HashMap<u8, u64>>,
    should_be_running: AtomicBool,
    game: Game
}

struct Status {
    gameboy: &'static safeboy::Gameboy,
    rng_hit: Rc<AtomicBool>,
    decision_made: Rc<AtomicU8>,
}

fn simulate(inner: Arc<SimulatorInner>) {
    let mut gameboy = safeboy::Gameboy::new(inner.model);
    gameboy.load_rom_from_buffer(inner.rom.as_slice());
    gameboy.set_turbo_mode(true, true);
    gameboy.set_rendering_disabled(false);

    macro_rules! make_gen2_rules {
        ($enemy_current_move_addr:expr, $enemy_current_move_num_addr:expr, $rand_low:expr, $rand_high:expr) => {
            gameboy.set_write_memory_callback(Some(|status, address, data| -> bool {
                if address == $enemy_current_move_addr && data != 0 {
                    let status = status.unwrap().downcast_mut::<Status>().unwrap();
                    let pc = status.gameboy.get_registers().pc as usize;
                    if pc > 0x4000 {
                        let offset = pc - 0x4000;
                        let (rom, bank) = status.gameboy.get_direct_access(DirectAccess::ROM);
                        let rom = &rom[0x4000 * bank as usize..];
                        let rom = rom.get(offset..offset+6);
                        let high = ($enemy_current_move_num_addr >> 8) as u8;
                        let low = ($enemy_current_move_num_addr & 0xFF) as u8;

                        // use a signature so ROM hacks can work provided RAM isn't moved around too much
                        if rom == Some(&[0x79, 0xEA, low, high, 0xC9, 0x91]) {
                            status.decision_made.swap(data, Ordering::Relaxed);
                        }
                    }
                }
                true
            }));
            gameboy.set_read_memory_callback(Some(|status, address, data| -> u8 {
                if address == $rand_low || address == $rand_high {
                    status.unwrap().downcast_mut::<Status>().unwrap().rng_hit.swap(true, Ordering::Relaxed);
                    return random();
                }
                data
            }));
        };
    }

    match inner.game {
        Game::Red | Game::Blue | Game::Yellow => {
            gameboy.set_write_memory_callback(Some(|status, address, data| -> bool {
                if address == 0xCCDD && data != 0 {
                    let status = status.unwrap().downcast_mut::<Status>().unwrap();
                    status.decision_made.swap(data, Ordering::Relaxed);
                }
                true
            }));
            gameboy.set_read_memory_callback(Some(|status, address, data| -> u8 {
                if address == 0xFFD3 || address == 0xFFD4 {
                    status.unwrap().downcast_mut::<Status>().unwrap().rng_hit.swap(true, Ordering::Relaxed);
                    return random();
                }
                data
            }));
        },
        Game::Gold | Game::Silver => {
            make_gen2_rules!(0xCBC2, 0xCBC7, 0xFFE3, 0xFFE4);
        }
        Game::Crystal => {
            make_gen2_rules!(0xC6E4, 0xC6E9, 0xFFE1, 0xFFE2);
        }
    }

    let mut trained = false;
    let mut last_save_state = inner.save_state.clone();

    loop {
        // We can load to the first instance of the random number generator if possible.
        gameboy.load_state_from_buffer(&last_save_state).unwrap();

        let rng_hit = Rc::new(AtomicBool::new(false));
        let decision_made = Rc::new(AtomicU8::new(0));

        let memes = Status {
            gameboy: unsafe { &*(&gameboy as *const _) },
            rng_hit: rng_hit.clone(),
            decision_made: decision_made.clone()
        };

        gameboy.set_user_data(Some(Box::new(memes)));

        let mut rapid_fire = 0u8;
        let mut odd_frame = false;

        let move_found = loop {
            if !inner.should_be_running.load(Ordering::Relaxed) {
                return;
            }

            // We found where the first random() call is!
            if !trained {
                if rng_hit.load(Ordering::Relaxed) {
                    trained = true;
                }
                else {
                    last_save_state = gameboy.read_save_state_to_vec();
                }
            }

            if odd_frame != gameboy.is_odd_frame() {
                rapid_fire = (rapid_fire + 1) % 6;
                gameboy.set_key_state(Key::A, rapid_fire < 3);
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
