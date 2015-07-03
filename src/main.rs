#![feature(convert)]

extern crate md_rel;
use std::env::args;

fn main() {
    for file in args() {
        let file = file.as_str();
        let _ = md_rel::transform_file(file);
    }
}
