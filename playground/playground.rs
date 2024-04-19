#[no_mangle]
fn left(x: i32, y: i32) -> i32 {
    x + y + 1
}

#[no_mangle]
fn right(x: i32, y: i32) -> i32 {
    x + 1 + y
}

fn main() {}
