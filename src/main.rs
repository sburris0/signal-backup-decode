use log::error;
use std::convert::TryInto;

mod Backups;
mod args;
mod decrypter;
mod display;
mod frame;
mod input;
mod output_raw;

fn run(config: &args::Config) -> Result<(), anyhow::Error> {
	// output
	let mut output = output_raw::Output::new(&config.path_output_main, true, true)?;

	// input
	let mut reader =
		input::InputFile::new(&config.path_input, &config.password, config.verify_mac)?;

	// progress bar
	let progress = display::Progress::new(
		reader.get_file_size(),
		reader.get_count_frame().try_into().unwrap(),
	);
	let progress_read = progress.clone();
	let progress_write = progress.clone();

	// channel to parallelize input reading / processing and output writing
	// and to display correct status
	let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel(10);

	let thread_input = std::thread::spawn(move || -> Result<(), anyhow::Error> {
		// we have to use a while let loop here because we want to access the reader object
		// in the loop. This does not work with a simple for loop.
		#[allow(clippy::while_let_on_iterator)]
		while let Some(frame) = reader.next() {
			match frame {
				Ok(x) => {
					// if we cannot send a frame, probably an error has occured in the
					// output thread. Thus, just shut down the input thread. We will print
					// the error in the output thread.
					if frame_tx.send(x).is_err() {
						break;
					}

					// forward progress bar if everything is ok
					progress_read.set_read_frames(reader.get_count_frame().try_into().unwrap());
					progress_read.set_read_bytes(reader.get_count_byte().try_into().unwrap());
				}
				Err(e) => {
					progress_read.finish_bytes();
					return Err(e);
				}
			}
		}

		progress_read.finish_bytes();
		Ok(())
	});

	let thread_output = std::thread::spawn(move || -> Result<(), anyhow::Error> {
		for received in frame_rx {
			match output.write_frame(received) {
				Ok(_) => progress_write
					.set_written_frames(output.get_written_frames().try_into().unwrap()),
				Err(e) => {
					progress_write.finish_frames();
					return Err(e);
				}
			}
		}

		progress_write.finish_frames();
		output.finish()?;
		Ok(())
	});

	progress.finish_multi();
	if let Err(e) = thread_input.join().unwrap() {
		error!("{:#}.", e);
	}
	if let Err(e) = thread_output.join().unwrap() {
		error!("{:#}.", e);
	}

	Ok(())
}

fn main() {
	// build config structure
	let config = args::Config::new().unwrap_or_else(|e| {
		eprintln!("Problem parsing arguments: {}.", e);
		std::process::exit(1);
	});

	simplelog::TermLogger::init(
		log::LevelFilter::Info,
		simplelog::Config::default(),
		simplelog::TerminalMode::Mixed,
	)
	.unwrap();

	if let Err(e) = run(&config) {
		error!("{:#}.", e);
		std::process::exit(1);
	}
}
