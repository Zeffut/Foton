//! Tiny executable fixture for `install-test.ps1`.

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("foton {}", env!("FOTON_INSTALL_TEST_VERSION"));
    }
}
