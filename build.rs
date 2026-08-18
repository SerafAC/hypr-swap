//! `wayland-scanner` expands the vendored protocol XML through a procedural macro in
//! `src/ui/shortcuts.rs`. Cargo cannot see that dependency by itself, so this script declares it:
//! editing the protocol re-runs the codegen.

const PROTOCOL: &str = "protocols/hyprland-global-shortcuts-v1.xml";

fn main() {
    println!("cargo::rerun-if-changed={PROTOCOL}");
}
