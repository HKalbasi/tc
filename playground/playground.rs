#[no_mangle]
fn left(x: i32, y: i32) -> i32 {
    x + y
}

#[no_mangle]
fn right(x: i32, y: i32) -> i32 {
    if x < y {
        x + y
    } else {
        y - x
    }
}

fn main() {}
