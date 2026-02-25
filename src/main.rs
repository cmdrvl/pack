use std::process;

fn main() {
    let exit_code = pack::run();
    process::exit(exit_code as i32);
}