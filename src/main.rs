mod data;

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs::read;
use std::io::{BufWriter, stdout, Write};
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use clap::Parser;
use console::Term;
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

#[derive(clap::Parser)]
struct Args {
    rom: PathBuf,
    save_state: PathBuf,

    #[arg(short = 'j', long = "jobs", help = "Number of CPU threads to use - by default, use all available CPU threads")]
    jobs: Option<NonZeroUsize>,

    #[arg(short = 't', long = "trials", help = "Number of trials to calculate - by default, it will keep going until you press CTRL-C")]
    trials: Option<NonZeroU64>,

    #[arg(short = 'q', long = "quiet", help = "Don't output anything until finished")]
    quiet: bool
}

fn main() {
    let args = Args::parse();

    let hashmap = Arc::new(Mutex::new(HashMap::<u8, u64>::new()));
    let sample_size = Arc::new(AtomicU64::new(0));
    let trials = args.trials.map(|t| t.get());

    let Ok(rom) = read(&args.rom) else {
        eprintln!("Failed to read ROM {}", args.rom.display());
        return;
    };

    let Ok(save_state) = read(&args.save_state) else {
        eprintln!("Failed to read save state {}", args.save_state.display());
        return;
    };

    let rom = Arc::new(rom);
    let save_state = Arc::new(save_state);

    let thread_count = args
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap())
        .get();

    let mut threads = Vec::with_capacity(thread_count);
    let start = Instant::now();

    let Ok(model) = safeboy::Gameboy::model_for_save_state(&save_state) else {
        eprintln!("Can't determine what type of Game Boy to emulate from the save state.");
        return;
    };

    let mut gameboy = safeboy::Gameboy::new(model);
    gameboy.load_rom_from_buffer(&rom);

    if gameboy.load_state_from_buffer(&save_state).is_err() {
        eprintln!("Unable to load save state!");
        return;
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
            eprintln!("Unknown game {n}. Aborting...");
            return;
        }
    };

    if !args.quiet {
        println!("Detected game as {game}...");
        println!("Starting with {thread_count} threads...");
    }

    let bail = Arc::new(AtomicBool::new(false));
    {
        let bail_copy = bail.clone();
        let _ = ctrlc::set_handler(move || { bail_copy.swap(true, Ordering::Relaxed); } );
    }

    for _ in 0..thread_count {
        let sample_size = sample_size.clone();
        let hashmap = hashmap.clone();
        let bail = bail.clone();
        let game = game.clone();
        let trials = trials.clone();
        let rom = rom.clone();
        let save_state = save_state.clone();
        threads.push(std::thread::spawn(move || {
            simulate(hashmap, sample_size, bail, game, trials, rom, save_state);
        }))
    }

    if !args.quiet {
        println!("Simulating... press CTRL-C to stop!");
    }

    let mut output = Term::stdout();

    loop {
        std::thread::sleep(Duration::from_millis(250));

        if !args.quiet {
            output.clear_line().unwrap();
        }

        let time_passed = Instant::now() - start;
        let seconds = time_passed.as_secs();

        let sec = seconds % 60;
        let min = seconds / 60;

        let sample_size = sample_size.load(Ordering::Relaxed);
        if bail.load(Ordering::Relaxed) || args.trials.is_some_and(|t| sample_size >= t.get()) {
            if sample_size == 0 {
                println!("Cancelled.");
                return;
            }
            println!("Finished {sample_size} trial{s} in {min}:{sec:02}", s=if sample_size == 1 { "" } else { "s" });
            break;
        }

        if args.quiet {
            continue;
        }

        if sample_size == 0 {
            if seconds < 5 {
                let _ = write!(&mut output, "Awaiting the AI's decision");

                let dots_to_show = (time_passed.as_millis() / 250) % 4;

                for _ in 0..dots_to_show {
                    let _ = write!(&mut output, ".");
                }
            }
            else {
                let _ = write!(&mut output, "No response in {seconds} seconds. Did you give me the right save state?");
            }
            continue;
        }

        let hashmap = hashmap.lock().unwrap();
        let mut items: Vec<(u8, u64)> = hashmap.iter().map(|(&a, &b)| (a, b)).collect();
        drop(hashmap);
        items.sort_by(|a, b| a.0.cmp(&b.0));

        let items_str = items.iter().map(|(index, count)| {
            let percent = 100.0 * *count as f64 / sample_size as f64;
            let Some(move_type) = data::MoveType::from_u8(*index) else {
                return (Cow::Owned(format!("UNK (0x{index:02X})")), count, percent);
            };
            (Cow::Borrowed(move_type.name()), count, percent)
        });

        let mut items_str = items_str.peekable();

        let columns = output.size().1 as u32;
        let temporary_allowance = (items_str.len().min(4) * 17) as u32;
        let columns = columns + temporary_allowance;

        if columns < 80 {
            while let Some((name, _, percent)) = items_str.next() {
                let _ = write!(&mut output, "{name} {percent:3.0}");
                if items_str.peek().is_some() {
                    let _ = write!(&mut output, " | ");
                }
            }
        }
        else if columns < 88 {
            while let Some((name, _, percent)) = items_str.next() {
                let _ = write!(&mut output, "{name} {percent:3.0}%");
                if items_str.peek().is_some() {
                    let _ = write!(&mut output, " | ");
                }
            }
        }
        else if columns < 92 {
            while let Some((name, _, percent)) = items_str.next() {
                let _ = write!(&mut output, "{name} {percent:3.1}%");
                if items_str.peek().is_some() {
                    let _ = write!(&mut output, " | ");
                }
            }
        }
        else if columns < 105 {
            while let Some((name, _, percent)) = items_str.next() {
                let _ = write!(&mut output, "{name}: {percent:5.1}%");
                if items_str.peek().is_some() {
                    let _ = write!(&mut output, " | ");
                }
            }
        }
        else if columns < 115 {
            let _ = write!(&mut output, "{sample_size:<7}");
            for (name, _, percent) in items_str {
                let _ = write!(&mut output, " | {name}: {percent:6.2}%");
            }
        }
        else {
            let _ = write!(&mut output, "{sample_size:<7}");
            for (name, _, percent) in items_str {
                let _ = write!(&mut output, " | {name}: {percent:6.2}%");
            }
            let _ = write!(&mut output, " | {min:02}:{sec:02}");
        }
    }

    drop(output);

    let mut writer = BufWriter::new(stdout().lock());
    let _ = writeln!(writer);
    let _ = writeln!(writer, "  MOVE            COUNT        %");
    let _ = writeln!(writer, "  ==============================");

    let hashmap = hashmap.lock().unwrap();
    let mut items: Vec<(u8, u64)> = hashmap.iter().map(|(&a, &b)| (a, b)).collect();
    drop(hashmap);
    items.sort_by(|a, b| a.0.cmp(&b.0));

    for (m, cnt) in items {
        let m = data::MoveType::from_u8(m).map(|m| m.name().to_owned()).unwrap_or(format!("UNK (0x{m:02X})"));
        let _ = writeln!(writer, "  {m:-12} {cnt:8} {:7.2}%", 100.0 * cnt as f64 / sample_size.load(Ordering::Relaxed) as f64);
    }

    let _ = writeln!(writer);

    for i in threads {
        let _ = i.join();
    }
}

struct Status {
    gameboy: &'static safeboy::Gameboy,
    rng_hit: Rc<AtomicBool>,
    decision_made: Rc<AtomicU8>,
}

fn simulate(hashmap: Arc<Mutex<HashMap<u8, u64>>>, count: Arc<AtomicU64>, bail: Arc<AtomicBool>, game: Game, trials: Option<u64>, rom: Arc<Vec<u8>>, save_state: Arc<Vec<u8>>) {
    let mut gameboy = safeboy::Gameboy::new(Model::CGBA);
    gameboy.load_rom_from_buffer(&rom);
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

    match game {
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
    let mut last_save_state = save_state.as_ref().to_owned();

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
        let move_found = loop {
            if bail.load(Ordering::Relaxed) {
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

            rapid_fire = (rapid_fire + 1) % 6;
            gameboy.set_key_state(Key::A, rapid_fire < 3);

            let result = decision_made.load(Ordering::Relaxed);
            if result != 0 {
                break result;
            }

            gameboy.run();
        };

        let new_count = count.fetch_add(1, Ordering::Relaxed);
        if trials.is_some_and(|t| new_count >= t) {
            count.fetch_sub(1, Ordering::Relaxed);
            return;
        }

        let mut hm = hashmap.lock().unwrap();
        if let Some(n) = hm.get_mut(&move_found) {
            *n += 1;
        }
        else {
            hm.insert(move_found, 1);
        }
    }
}
