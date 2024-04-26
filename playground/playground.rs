#[no_mangle]
fn left(x: i32, y: i32) -> i32 {
    if x < y {
        bar();
    } else {
        foo();
    }
    1
}

#[inline(never)]
fn foo() {
    println!("salam");
}

#[inline(never)]
fn bar() {
    println!("salamss");
}

#[no_mangle]
fn right(x: i32, y: i32) -> i32 {
    if 1 + x < 1 + y {
        bar();
    } else {
        foo();
    }
    1
}

fn main() {
    let x = -1610612736i32;
    let y = 0x04002000;
    let return_left = left(x, y);
    let return_right = right(x, y);
    println!("{:#08x}", return_left);
    println!("{:#08x}", return_right);
}
