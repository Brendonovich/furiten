//! `FURITEN_CHANNEL=dev` at build time makes a dev build: `Furiten Dev.app`, with its own data home and preferences so
//! it runs beside Furiten. bundle.sh passes it as `--config env.FURITEN_CHANNEL=dev`, so Cargo rebuilds when it changes.

fn main() {
    println!("cargo:rerun-if-env-changed=FURITEN_CHANNEL");
    let dev = std::env::var("FURITEN_CHANNEL").is_ok_and(|channel| channel == "dev");
    println!("cargo:rustc-env=FURITEN_DEV_BUILD={}", u8::from(dev));
}
