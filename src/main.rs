use anyhow::{anyhow, Error};
use human_size::{Byte, Size};
use sketch_duplicates::DuplicatesSketch;
use std::{
    fs::File,
    io::{stdin, stdout, BufRead, BufReader, BufWriter, Read, Write},
    path::PathBuf,
};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "dubs-sketch",
    about = "Find duplicate lines probabilistically"
)]
enum Opt {
    #[structopt(about = "Build a sketch from lines in standard input")]
    Build {
        #[structopt(
            short,
            long,
            default_value = "2",
            about = "Number of probes in sketch. Larger values are more precise, but slower"
        )]
        probes: u32,

        #[structopt(
            short,
            long,
            default_value = "8MiB",
            about = "Minimum size of the sketch. Actual size will be nearest power of two larger than this."
        )]
        size: Size,

        #[structopt(
            short = "0",
            long,
            about = "Use NULL bytes as line delimiters instead of newlines."
        )]
        zero_terminated: bool,
    },
    #[structopt(about = "Combine multiple sketches into one.")]
    Combine,
    #[structopt(about = "Remove most lines that do not have duplicates.")]
    Filter {
        #[structopt(
            about = "Sketch to filter by."
        )]
        sketch: PathBuf,
    
        #[structopt(
            short = "0",
            long,
            about = "Use NULL bytes as line delimiters instead of newlines."
        )]
        zero_terminated: bool
    },
}

fn combine_sketches(mut r: impl Read) -> Result<DuplicatesSketch, Error> {
    let mut sketch: Option<DuplicatesSketch> = None;

    while let Some(to_merge) = DuplicatesSketch::deserialize(&mut r)? {
        match sketch {
            Some(ref mut sketch) => {
                if !sketch.is_compatible(&to_merge) {
                    return Err(anyhow!("Incompatible sketches"));
                }

                sketch.merge(&to_merge);
            }
            None => sketch = Some(to_merge),
        };
    }

    sketch.ok_or_else(|| anyhow!("No sketches in input"))
}

fn main() -> Result<(), Error> {
    let opts = Opt::from_args();

    let stdin = stdin();
    let mut stdin = stdin.lock();
    let stdout = stdout();
    let mut stdout = BufWriter::new(stdout.lock());

    match opts {
        Opt::Build { probes, size, zero_terminated } => {
            if probes == 0 {
                return Err(anyhow!("Number of probes cannot be 0"));
            }

            let size = size.into::<Byte>().value() as usize;
            let mut sketch = DuplicatesSketch::new(probes, size);

            let sep = if zero_terminated { 0 } else { b'\n' };
            let mut buf = Vec::new();
            while stdin.read_until(sep, &mut buf)? != 0 {
                sketch.insert(&buf);
                buf.clear();
            }

            sketch.serialize(stdout)?;
        }
        Opt::Combine => {
            combine_sketches(&mut stdin)?.serialize(stdout)?;
        }
        Opt::Filter { sketch, zero_terminated } => {
            let sketch = combine_sketches(BufReader::new(File::open(sketch)?))?;

            let sep = if zero_terminated { 0 } else { b'\n' };
            let mut buf = Vec::new();
            while stdin.read_until(sep, &mut buf)? != 0 {
                if sketch.has_duplicate(&buf) {
                    stdout.write_all(&buf)?;
                }
                buf.clear();
            }
        }
    }

    Ok(())
}