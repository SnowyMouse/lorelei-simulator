use std::borrow::Cow;
use std::fs::read;
use std::io::{BufWriter, stdout, Write};
use std::num::{NonZeroU64, NonZeroUsize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use clap::Parser;
use console::Term;
use lorelei_simulator::{move_name, Simulator};

fn main() {
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

    let args = Args::parse();
    let trials = args.trials.map(|t| t.get());

    let Ok(rom) = read(&args.rom) else {
        eprintln!("Failed to read ROM {}", args.rom.display());
        return;
    };

    let Ok(save_state) = read(&args.save_state) else {
        eprintln!("Failed to read save state {}", args.save_state.display());
        return;
    };

    let mut simulator = match Simulator::new_from_vec(rom, save_state, trials) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Failed to load simulator: {e}");
            return;
        }
    };

    let thread_count = args
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().unwrap());

    simulator.start(thread_count);

    let bail = {
        let bail = Arc::new(AtomicBool::new(false));
        let bail_copy = bail.clone();
        let _ = ctrlc::set_handler(move || { bail_copy.swap(true, Ordering::Relaxed); } );
        bail
    };

    if !args.quiet {
        println!("Simulating... press CTRL-C to stop!");
    }

    let mut output = Term::stdout();
    let start = Instant::now();

    loop {
        std::thread::sleep(Duration::from_millis(250));

        let bailing = bail.load(Ordering::Relaxed);
        if bailing {
            simulator.stop();
        }

        if !args.quiet {
            output.clear_line().unwrap();
        }

        let hashmap = simulator.results();
        let time_passed = Instant::now() - start;
        let seconds = time_passed.as_secs();

        let sec = seconds % 60;
        let min = seconds / 60;

        let mut sample_size = 0;
        for i in &hashmap {
            sample_size += *i.1
        };

        if !simulator.is_running() {
            if bailing && sample_size == 0 {
                output.clear_line().unwrap();
                println!("Cancelled; no trials recorded in {min}:{sec:02}");
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

        let mut items: Vec<(u8, u64)> = hashmap.iter().map(|(&a, &b)| (a, b)).collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));

        let items_str = items.iter().map(|(index, count)| {
            let percent = 100.0 * *count as f64 / sample_size as f64;
            let Some(move_name) = move_name(*index) else {
                return (Cow::Owned(format!("UNK (0x{index:02X})")), count, percent);
            };
            (Cow::Borrowed(move_name), count, percent)
        });

        let mut items_str = items_str.peekable();

        // If there aren't as many items to display, lower the threshold
        let columns = output.size().1 as u32;
        let extra_room = ((4 - items_str.len().min(4)) * 17) as u32;
        let columns = columns + extra_room;

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

    let hashmap = simulator.results();
    let mut sample_size = 0;
    for i in &hashmap {
        sample_size += *i.1
    };

    let mut writer = BufWriter::new(stdout().lock());
    let _ = writeln!(writer);
    let _ = writeln!(writer, "MOVE            COUNT        %");
    let _ = writeln!(writer, "==============================");

    let mut items: Vec<(u8, u64)> = hashmap.iter().map(|(&a, &b)| (a, b)).collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    for (m, cnt) in items {
        let m = move_name(m).map(|m| m.to_owned()).unwrap_or(format!("UNK (0x{m:02X})"));
        let _ = writeln!(writer, "{m:-12} {cnt:8} {:7.2}%", 100.0 * cnt as f64 / sample_size as f64);
    }

    let _ = writeln!(writer);
}
