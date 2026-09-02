fn main() {
    pkg_config::Config::new()
        .atleast_version("0.1")
        .probe("libside")
        .expect("failed to locate libside with pkg-config");

    let lttng_ust = pkg_config::Config::new()
        .atleast_version("0.1")
        .cargo_metadata(false)
        .probe("lttng-ust")
        .expect("failed to locate lttng-ust with pkg-config");

    for link_path in lttng_ust.link_paths {
        println!("cargo:rustc-link-search=native={}", link_path.display());
    }

    println!("cargo:rustc-link-arg=-Wl,--push-state");
    println!("cargo:rustc-link-arg=-Wl,--no-as-needed");

    for lib in lttng_ust.libs {
        println!("cargo:rustc-link-lib={lib}");
    }

    println!("cargo:rustc-link-arg=-Wl,--pop-state");
}
