#![windows_subsystem = "windows"]

mod api;
mod cache;
mod config;
mod models;
mod oauth;
mod speech;
mod ui;

fn main() {
    ui::run();
}
