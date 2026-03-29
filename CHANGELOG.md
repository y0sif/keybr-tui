# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-29

### Added

- Adaptive phonetic text generation using Markov chains (faithful port of the keybr.com algorithm)
- Per-key confidence tracking with exponential smoothing of reaction times
- Progressive letter unlocking based on performance against target speed
- Focus key system that biases text generation toward your weakest letter
- Backspace support with two error modes: forgive mistakes and stop on error
- Real-time WPM and accuracy display during typing sessions
- Lesson summary screen showing WPM, accuracy, newly unlocked letters, and weakest keys
- Main menu with navigation to typing practice, progress view, and settings
- Progress view displaying per-key statistics (speed, confidence, attempts, errors)
- Settings screen for adjusting target WPM, error mode, and fragment length
- Persistent stats saved automatically between sessions (JSON format)
- Persistent configuration via TOML config file
- CLI arguments: `--target-wpm`, `--error-mode`, `--reset`, `--data-dir`
- Minimalist terminal UI built with ratatui and crossterm
- ANSI-only color palette for universal terminal compatibility
