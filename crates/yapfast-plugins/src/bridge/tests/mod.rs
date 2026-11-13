#[allow(unused_imports)] // the harness every part shares
use super::*;

use crate::grants::Grants;
#[allow(unused_imports)] // used by the parts, not this file
use crate::network::{ConnectPlan, NetworkGate, Resolver, Served, Transport};
// used by the parts, not this file
#[allow(unused_imports)]
use futures_util::future::BoxFuture;
// used by the parts, not this file
#[allow(unused_imports)]
use futures_util::stream::{self, StreamExt};
// used by the parts, not this file
#[allow(unused_imports)]
use std::net::IpAddr;
#[allow(unused_imports)] // used by the parts, not this file
use yapfast_plugin_api::PluginManifest;
use yapfast_plugin_api::event::Network;

mod support_01;
mod support_02;
mod support_03;
mod support_world;
#[allow(unused_imports)] // the harness every part shares
use support_01::*;
#[allow(unused_imports)] // the harness every part shares
use support_02::*;
#[allow(unused_imports)] // the harness every part shares
use support_03::*;
#[allow(unused_imports)] // the harness every part shares
use support_world::*;

mod part_01;
mod part_02;
mod part_03;
mod part_04;
mod part_05;
mod part_06;
mod part_07;
mod part_08;
mod part_09;
mod part_10;
mod part_11;
