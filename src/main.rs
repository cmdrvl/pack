use std::process;

fn main() {
    let code = pack::run();
    process::exit(i32::from(code));
}
