use interprocess::local_socket;
use std::{env, io::Write, process::Command};

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    let name = env::var("__CARGO_GEAR_SOCKET_NAME").unwrap();
    let mut stream = local_socket::LocalSocketStream::connect(name).unwrap();
    write!(&mut stream, "{}", args.join(" ")).unwrap();
    drop(stream);

    let rustc = args.remove(0);
    let status = Command::new(rustc).args(args).status().unwrap();
    assert!(status.success());
}
