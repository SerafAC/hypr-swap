//! Shared machinery for the end-to-end suite.
//!
//! Every E2E test drives the application through its real external interface: a nested Hyprland
//! running the user's documented bind lines, real key events injected through
//! `virtual-keyboard-unstable-v1`, and assertions made by asking that compositor what happened
//! over its own IPC socket (research.md R14).
//!
//! The documented substitutions are headless outputs for physical monitors, `foot` for arbitrary
//! user applications, and — confined to the rollback tests — the environment-gated fault
//! injection in `hypr::ipc`.

#![allow(dead_code)]

pub mod clients;
pub mod fixtures;
pub mod harness;
pub mod keyboard;
pub mod notify;
