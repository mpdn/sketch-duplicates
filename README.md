# sketch-duplicates

Find duplicate lines probabilistically.

## Motivation

Let's say you have a directorty of gzipped text files that you want to check for duplicate lines in.
The usual way to do this might look something like this:

```shell
zcat *.gz | sort | uniq -d
```

The problem with this, is that it can become very slow for large files. `sketch-duplicates` provides
a way to remove *most* unique lines, leaving *mostly* duplicate lines in the output.
`sketch-duplicates` is probabilistic and is therefore not guarenteed to remove *all* unique lines.
It is therefore still necessary to have a `sort | uniq -d` in the end but this will be much faster
due to the input having most unique lines removed.

The above can example can be written to use a sketch like this:

```shell
zcat *.gz | sketch-duplicates build > sketch
zcat *.gz | sketch-duplicates filter sketch | sort | uniq -d
```

Multiple sketches can be combined using `sketch-duplicates combine`. This can be used to parallelize
the construction of the sketch (here using GNU Parallel):

```shell
echo *.gz | parallel 'zcat {} | sketch-duplicates build' | sketch-duplicates combine > sketch
echo *.gz | parallel 'zcat {} | sketch-duplicates filter sketch' | sort | uniq -d
```

## Options

- `-s`, `--size`: Size of the sketch. Increasing this improves filtering accuracy but consumes more
  memory. This is set to a conservative default of 8MiB and can often be increased depending on the
  specific use case.
- `-p`, `--probes`: Number of probes to do in the sketch.
- `-0`, `--zero-terminated`: Use NULL bytes as line delimiters. 

## Install

Install Cargo (eg. using [rustup](https://www.rust-lang.org/tools/install)), then run
`cargo install sketch-duplicates`.