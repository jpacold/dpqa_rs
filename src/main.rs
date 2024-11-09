pub mod dpqa;

use dpqa::DPQA;

fn main() {
    let compiler = DPQA::new(3, 2);
    println!("{}", compiler);
}
