//! The exploratory tester: seeded walks over an app's screens that report
//! crashes, error alerts, dead ends and accessibility problems, each with a
//! `forge ui run` replay file (docs/forge-explore.md), and the focus that
//! aims a walk at what a change touched (docs/forge-m6.md, M6.2).
//!
//! It drives the phone through ixOS's `forge-uitest` (the `Driver`, the
//! any-app policy, test scripts). [`model`] is a screen-graph model of an
//! app, so a walk can run on Linux without a phone.

pub mod explore;
pub mod focus;
pub mod model;
