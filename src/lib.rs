#![feature(plugin)]
#![feature(convert)]
#![feature(slice_chars)]
#![plugin(regex_macros)]
// #![feature(collections)]

extern crate regex;
#[macro_use] extern crate try_or;

use std::fs::File;
use std::io::Error;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;


#[cfg(test)]
mod test;

#[derive(Debug, PartialEq)]
pub enum LineType {
    WholeFile(String), // (filename)
    Section(String, String), // (filename, sectionname)
    Lines(String, usize, usize) // (filename, startline, endline)
}

#[derive(Debug)]
pub enum MdError {
    OpenRead(Error),
    OpenWrite(Error),
    Source(Error),
    Import(Error),
    Output(Error),
    NonMatchingCode(String),
    SectionNotFound(String, usize),
    InvalidLineChunk(String),
    FileTooSmall(String, usize)
}

pub type MdResult<A> = Result<A, MdError>;

pub fn detect_type(line: &str) -> Option<LineType> {
    let file = regex!(r"\^code\( *([^, ]+) *\)");
    let section = regex!(r"\^code\( *([^, ]+) *, *([a-zA-Z]+) *\)");
    let lines = regex!(r"\^code\( *([^, ]+) *, *([0-9]+) *, *([0-9]+) *\)");

    if file.is_match(line) {
        let capture = file.captures(line).unwrap();
        Some(LineType::WholeFile(capture.at(1).unwrap().to_string()))
    } else if section.is_match(line) {
        let capture = section.captures(line).unwrap();
        Some(LineType::Section(capture.at(1).unwrap().to_string(), capture.at(2).unwrap().to_string()))
    } else if lines.is_match(line) {
        let capture = lines.captures(line).unwrap();
        let (start, end) = (capture.at(2).unwrap().parse(), capture.at(3).unwrap().parse());
        match (start, end) {
            (Ok(s), Ok(e)) => Some(LineType::Lines(capture.at(1).unwrap().to_string(), s, e)),
            _ => None
        }
    } else {
        None
    }
}

pub fn rewrite<R, W, F> (linetype: LineType, fetch: F, out_buffer: &mut BufWriter<W>) -> MdResult<()>
where F: Fn(&str) -> MdResult<BufReader<R>>,
      R: Read, W: Write {
    let file = match linetype {
        LineType::WholeFile(ref s) => s,
        LineType::Section(ref s, _) => s,
        LineType::Lines(ref s, _, _) => s,
    }.as_str();

    let reader = try_or!(fetch(file));

    match linetype {
        LineType::WholeFile(_) => {
            for line in reader.lines() {
                let line = try_or!(line, MdError::Import);
                let line = line.as_bytes();
                try_or!(out_buffer.write_all(line), MdError::Output);
            }
            try_or!(out_buffer.write_all(b"\n"), MdError::Output);
            Ok(())
        }
        LineType::Section(_, section_name) => {
            let mut scanning = false;
            for line in reader.lines() {
                let line = try_or!(line, MdError::Import);
                let trimmed = line.as_str().trim_left_matches(' ');
                let prelude = "// section ";
                if trimmed.starts_with(prelude) {
                    let name = trimmed
                        .slice_chars(prelude.len(),trimmed.len())
                        .trim_matches(' ')
                        .trim_matches('\n');
                    if scanning {
                        break;
                    } else {
                        if name == section_name.as_str() {
                            scanning = true;
                        }
                    }
                } else if scanning {
                    let line = line.as_bytes();
                    try_or!(out_buffer.write_all(line), MdError::Output);
                }
            }
            Ok(())
        }
        LineType::Lines(_, start, end) => {
            for line in reader.lines().skip(start).take(end - start + 1) {
                let line = try_or!(line, MdError::Import);
                let line = line.as_bytes();
                try_or!(out_buffer.write_all(line), MdError::Output);
            }
            Ok(())
        }
    }
}

pub fn process_file<R, W, F> (in_buffer: &mut BufReader<R>,
                              out_buffer: &mut BufWriter<W>,
                              fetch: F) -> MdResult<()>
where R: Read, W: Write, F: Fn(&str) -> MdResult<BufReader<R>> {
    let in_buffer = in_buffer;
    let out_buffer = out_buffer;
    for line in in_buffer.lines() {
        let line = try_or!(line, MdError::Source);
        let line = line.as_str();
        if line.starts_with("^code") {
            match detect_type(line) {
                Some(typ) => {
                    try_or!(out_buffer.write(b"```rust\n"), MdError::Output);
                    try_or!(rewrite(typ, |a| fetch(a), out_buffer));
                    try_or!(out_buffer.write(b"```\n"), MdError::Output);
                }
                None => {

                }
            }
        } else {
            try_or!(out_buffer.write(line.as_bytes()), MdError::Output);
        }
    }
    Ok(())
}

pub fn transform_file(source: &str) -> MdResult<()> {
    let out_name = {
        let mut base;
        if source.ends_with(".dev.md") {
            base = String::from(source.slice_chars(0,source.len() - 7));
        } else {
            base = String::from(source);
        }
        base.push_str(".md");
        base
    };

    let in_path = PathBuf::from(source);
    let out_path = Path::new(out_name.as_str());
    let mut relative_path = in_path.clone();
    relative_path.pop();

    let read_file = try_or!(File::open(&in_path), MdError::OpenRead);
    let write_file = try_or!(File::create(&out_path), MdError::OpenWrite);

    let mut read_buffer = BufReader::new(read_file);
    let mut write_buffer = BufWriter::new(write_file);

    process_file(&mut read_buffer, &mut write_buffer, |extra| {
        let mut temp = relative_path.clone();
        temp.push(extra);
        let source_file = try_or!(File::open(&temp), MdError::OpenRead);
        Ok(BufReader::new(source_file))
    })
}
