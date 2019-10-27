#[macro_use] extern crate clap;
extern crate chrono;
extern crate indicatif;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{
    Read, Write,
    BufReader, BufWriter,
    ErrorKind};
use std::io;

const DEFAULT_BUF_SIZE: usize = 65536;

fn main() {
    let matches = clap_app!(pv =>
        (version: "0.1.0")
        (author: "Sean Gallagher <stgallag@gmail.com>")
        (about: "A progress bar and flow rate meter for Unix pipes, (a rust clone, built from clap and indicatif)")
        (@arg size: -s --size +takes_value "Set estimated data size to SIZE bytes")
        (@arg timer: -t --timer "Show elapsed time")
        (@arg width: -w --width +takes_value "Width of the progressbar (default: max)")
        (@arg bytes: -b --bytes "Show number of bytes transferred")
        (@arg rate: -r --rate "Show data transfer rate counter")
        (@arg average_rate: -a --("average-rate") "Show data transfer average rate counter (same as rate in this implementation, for now)")
        (@arg eta: -e --eta "Show estimated time of arrival (completion)")
        (@arg line_mode: -l --("line-mode") "Count lines instead of bytes")
        (@arg null: --null "Lines are null-terminated") // TODO: need to support -0
        (@arg skip_input_errors: -E --("skip-errors") "Skip read errors in input")
        (@arg skip_output_errors: --("skip-output-errors") "Skip read errors in output")
        //(@arg INPUT: ... "Input filenames")

        // These are not really a priority
        (@arg buffer_percent: -T --("buffer-percent") "Ignored for compatibility")
        (@arg buffer_size: -B --("buffer-size") +takes_value "Ignored for compatibility")
        (@arg quiet: -q --quiet "Ignored for compatibility; if you want \"quiet\", don't use pv")
        (@arg progress: -p --progress "Ignored for compatibility; this implementation always shows the progressbar")
    ).get_matches();
    PipeView {
        source: Box::new(BufReader::new(io::stdin())), // Source
        sink: Box::new(BufWriter::new(io::stdout())),   // Sink
        progress: PipeView::progress_from_options(
            matches.value_of("size").and_then(|x| x.parse().ok()), // Estimated size
            matches.is_present("timer"),        // Whether to show Elapsed Time
            matches.value_of("width").and_then(|x| x.parse().ok()), // Progressbar width
            matches.is_present("bytes"),        // Whether to show transferred Bytes
            matches.is_present("eta"),          // Whether to show ETA
            matches.is_present("rate") || matches.is_present("average_rate"),         // Whether to show the rate. TODO: Show average rate separately
            matches.is_present("line_mode"),    // Whether to work by lines instead
        ),
        line_mode: if matches.is_present("line_mode") {
            LineMode::Line(if matches.is_present("null") { 0 } else { 10 }) // default to unix newline
        } else {
            LineMode::Byte
        },
        skip_input_errors: matches.is_present("skip_input_errors"),
        skip_output_errors: matches.is_present("skip_output_errors")
    }.pipeview().unwrap();
}

enum LineMode {
    Line(u8),
    Byte
}
struct PipeView {
    source: Box<dyn Read>,
    sink: Box<dyn Write>,
    progress: ProgressBar,
    line_mode: LineMode,
    skip_input_errors: bool,
    skip_output_errors: bool
}

impl PipeView {
    /// Set up the progress bar from the parsed CLI options
    fn progress_from_options(
        len: Option<u64>,
        show_timer: bool,
        width: Option<usize>,
        show_bytes: bool,
        show_eta: bool,
        show_rate: bool,
        line_mode: bool
    ) -> ProgressBar {
        // What to show, from left to right, in the progress bar
        let mut template = vec![];
        if show_timer {
            template.push("{elapsed_precise}".to_string());
        }

        match width {
            Some(x) => template.push(format!("{{bar:{}}} {{percent}}", x)),
            None => template.push("{wide_bar} {percent}".to_string())
        }

        // Choose whether you want bytes or plain counts on several fields
        let (pos_name, len_name, per_sec_name) = if line_mode {
            ("{pos}", "{len}", "{per_sec}")
        } else {
            ("{bytes}", "{total_bytes}", "{bytes_per_sec}")
        };

        // Put the transferred and total together so they don't have a space
        if show_bytes && len.is_some() {
            template.push(format!("{}/{}", pos_name, len_name));
        } else if show_bytes {
            template.push(pos_name.to_string());
        }

        if show_rate {
            template.push(per_sec_name.to_string());
        }
        
        if show_eta {
            template.push("{eta_precise}".to_string());
        }

        let mut style = match len {
            Some(_x) => ProgressStyle::default_bar(),
            None => ProgressStyle::default_spinner()
        };

        // Okay, that's all fine and dandy but if they don't specify anything,
        // we should have a nicer default than all empty
        if !(show_timer || show_bytes || show_rate || show_eta) {
            style = style.template(&format!(
                "{{elapsed}} {{wide_bar}} {{percent}} {}/{} {} {{eta}}",
                pos_name, len_name, per_sec_name)
            );
        } else {
            style = style.template(&template.join(" "));
        }

        let progress = match len {
            Some(x) => ProgressBar::new(x),
            None => ProgressBar::new_spinner()
        };
        
        progress.set_style(style);
        progress
    }

    fn pipeview(&mut self) -> Result<u64, Box<dyn ::std::error::Error>> {
        // Essentially std::io::copy
        let mut buf = [0; DEFAULT_BUF_SIZE];
        let mut written : u64 = 0;
        loop {
            // Always skip interruptions, maybe skip other errors
            // Also maybe finish if we read nothing
            let len = match self.source.read(&mut buf) {
                Ok(0) => return Ok(written),
                Ok(len) => len,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) if self.skip_input_errors => continue,
                Err(e) => return Err(e.into()),
            };

            // Maybe skip output errors
            match self.sink.write_all(&buf[..len]) {
                Ok(_) => (),
                Err(_) if self.skip_output_errors => continue,
                Err(e) => return Err(e.into())
            };
            match self.line_mode {
                LineMode::Line(delim) => self.progress.inc(buf[..len].iter().filter(|b| **b == delim).count() as u64),
                LineMode::Byte => self.progress.inc(len as u64)
            };
            written += len as u64;
        }
    }
}
