fn main() {
    // test alloc
    let mut data = Vec::new();
    data.push(5);
    println!("Allocated data: {:?}", data);
    println!("This is a compatibility layer for old syscalls. It should not be run directly.");
}