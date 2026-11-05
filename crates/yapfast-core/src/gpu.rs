//! Which of a machine's graphics adapters Yapfast makes its device on, and the
//! setting that decides it. A machine with a graphics card and a processor's
//! GPU has two adapters, and which one the device is made on decides which
//! hardware encoder is reachable — NVENC on NVIDIA's card, Quick Sync on
//! Intel's — and what a captured screen costs to reach it, a copy of every
//! frame unless the capture and the encoder are on the same adapter.
//!
//! Dedicated is the default: a game runs on the card, and NVENC is what
//! streaming was written for. The choice is made once, when the device is
//! made, so it costs nothing per frame.
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Which of a machine's graphics adapters to make Yapfast's device on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    /// The card with memory of its own, NVIDIA's or AMD's.
    #[default]
    Dedicated,
    /// The GPU built into the processor, Intel's.
    Integrated,
}

/// DXGI's vendor ids for the makers of those GPUs.
const NVIDIA: u32 = 0x10DE;
const AMD: u32 = 0x1002;
const INTEL: u32 = 0x8086;

/// Where an adapter stands under a [`Preference`]. Ranks order best first, so
/// a loop over the adapters takes the smallest; an adapter with no rank is
/// never made a device on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The adapter the preference asks for.
    Preferred,
    /// A hardware adapter of another kind, made the device on when the
    /// preferred kind isn't there, which leaves a machine with one GPU
    /// exactly as it was.
    Fallback,
}

/// DXGI's name for an adapter: locally unique, the same across runs and
/// enumerations, and what the device is asked for.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Luid {
    pub low: u32,
    pub high: i32,
}

/// What an enumerated adapter offers towards choosing it, as
/// `DXGI_ADAPTER_DESC1` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// Which adapter this is, to make the device on it.
    pub luid: Luid,
    /// `VendorId`: who makes the GPU.
    pub vendor: u32,
    /// `DedicatedVideoMemory`: the memory it has of its own, in bytes.
    pub dedicated_memory: u64,
    /// `SharedSystemMemory`: how much of the system's memory it will use, in
    /// bytes. It says nothing about which kind of GPU a card is — an
    /// integrated one reports up to half of RAM here — so it doesn't decide
    /// anything.
    pub shared_memory: u64,
    /// `DXGI_ADAPTER_FLAG_SOFTWARE`: the Basic Render Driver, or a driver
    /// small enough to be run by the processor, drawing what a GPU would.
    pub software: bool,
}

impl Preference {
    /// Where `adapter` stands under this preference, or `None` for a software
    /// adapter, which is never made a device on: it has no encoder and no
    /// texture of its own.
    ///
    /// The kind of GPU is read from the vendor and never from the memory,
    /// because memory doesn't tell them apart: an Intel UHD reports a nominal
    /// share of dedicated memory beside half of RAM in `SharedSystemMemory`,
    /// and Arc, Intel's own card, reports the same shape with gigabytes of
    /// its own. A threshold on either would call the UHD a card or the Arc an
    /// integrated GPU, by how much system memory the machine has.
    pub fn rank(self, adapter: &Adapter) -> Option<Rank> {
        if adapter.software {
            return None;
        }
        let card = matches!(adapter.vendor, NVIDIA | AMD);
        let wanted = match self {
            Preference::Dedicated => card,
            Preference::Integrated => adapter.vendor == INTEL,
        };
        Some(if wanted {
            Rank::Preferred
        } else {
            Rank::Fallback
        })
    }
}

/// The adapter to make the device on: the best rank under `preference`, and of
/// two ranked alike, the one with more memory of its own. That only sorts two
/// adapters of the same kind, and anything still tied keeps the order DXGI
/// listed them in, which starts with the adapter the desktop shows on.
///
/// `None` only when there is no hardware adapter at all, so a machine with one
/// GPU behaves as it did before this setting existed.
pub fn choose(
    preference: Preference,
    adapters: impl IntoIterator<Item = Adapter>,
) -> Option<Adapter> {
    adapters
        .into_iter()
        .filter_map(|adapter| preference.rank(&adapter).map(|rank| (rank, adapter)))
        .min_by(|(rank, adapter), (best, best_adapter)| {
            rank.cmp(best).then(
                adapter
                    .dedicated_memory
                    .cmp(&best_adapter.dedicated_memory)
                    .reverse(),
            )
        })
        .map(|(_, adapter)| adapter)
}

static PREFERENCE: AtomicU8 = AtomicU8::new(Preference::Dedicated as u8);

/// Chooses the GPU every stream's device is made on. A stream going on keeps
/// the device it started with, so the choice takes effect at the next one.
pub fn set(preference: Preference) {
    PREFERENCE.store(preference as u8, Ordering::Relaxed);
}

pub fn preference() -> Preference {
    match PREFERENCE.load(Ordering::Relaxed) {
        1 => Preference::Integrated,
        _ => Preference::Dedicated,
    }
}

#[cfg(test)]
mod tests {
    use super::{AMD, Adapter, INTEL, Luid, NVIDIA, Preference, Rank, choose};

    const GIB: u64 = 1 << 30;

    /// A card as DXGI lists one.
    fn card(luid: u32, vendor: u32, dedicated: u64) -> Adapter {
        Adapter {
            luid: Luid { low: luid, high: 0 },
            vendor,
            dedicated_memory: dedicated,
            shared_memory: 0,
            software: false,
        }
    }

    /// A GPU on the processor: a nominal share of memory of its own, and up to
    /// half of RAM to draw from.
    fn on_die(luid: u32, vendor: u32) -> Adapter {
        Adapter {
            shared_memory: 8 * GIB,
            ..card(luid, vendor, 128 << 20)
        }
    }

    fn software() -> Adapter {
        Adapter {
            software: true,
            ..card(9, 0x1414, 0)
        }
    }

    fn pick(preference: Preference, adapters: &[Adapter]) -> Option<u32> {
        choose(preference, adapters.iter().copied()).map(|adapter| adapter.luid.low)
    }

    /// The UI's layout.json cut to this one field, which a file written before
    /// the setting has no key for.
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Layout {
        gpu: Preference,
    }

    #[test]
    fn dedicated_is_the_default_and_layout_json_says_so_lowercase() {
        assert_eq!(Preference::default(), Preference::Dedicated);
        // The words the file holds, lowercase as `noise_suppression` holds
        // `"rnnoise"`.
        assert_eq!(
            serde_json::to_string(&Preference::Dedicated).unwrap(),
            "\"dedicated\""
        );
        assert_eq!(
            serde_json::to_string(&Preference::Integrated).unwrap(),
            "\"integrated\""
        );
        assert_eq!(
            serde_json::from_str::<Layout>("{\"gpu\":\"integrated\"}")
                .unwrap()
                .gpu,
            Preference::Integrated
        );
        // A file from before the setting is on the card, as it behaved then.
        assert_eq!(
            serde_json::from_str::<Layout>("{}").unwrap().gpu,
            Preference::Dedicated
        );
    }

    #[test]
    fn an_nvidia_card_and_intels_igpu_swap_places_with_the_preference() {
        let adapters = [card(1, NVIDIA, 24 * GIB), on_die(2, INTEL)];
        assert_eq!(
            Preference::Dedicated.rank(&adapters[0]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Dedicated.rank(&adapters[1]),
            Some(Rank::Fallback)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[1]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[0]),
            Some(Rank::Fallback)
        );
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(2));
    }

    #[test]
    fn amds_card_and_intels_igpu_swap_places_the_same_way() {
        let adapters = [on_die(1, INTEL), card(2, AMD, 16 * GIB)];
        assert_eq!(
            Preference::Dedicated.rank(&adapters[1]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[0]),
            Some(Rank::Preferred)
        );
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(2));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(1));
    }

    #[test]
    fn one_gpu_is_chosen_whichever_kind_it_is_and_whichever_is_preferred() {
        let solo_card = [card(1, NVIDIA, 24 * GIB)];
        let solo_igpu = [on_die(1, INTEL)];
        let solo_amd = [card(1, AMD, 16 * GIB)];
        for preference in [Preference::Dedicated, Preference::Integrated] {
            assert_eq!(pick(preference, &solo_card), Some(1), "{preference:?}");
            assert_eq!(pick(preference, &solo_igpu), Some(1), "{preference:?}");
            assert_eq!(pick(preference, &solo_amd), Some(1), "{preference:?}");
        }
        // The one it isn't the preference for has no better rank, so it is
        // what runs, rather than nothing.
        assert_eq!(
            Preference::Integrated.rank(&solo_card[0]),
            Some(Rank::Fallback)
        );
        assert_eq!(
            Preference::Dedicated.rank(&solo_igpu[0]),
            Some(Rank::Fallback)
        );
    }

    #[test]
    fn a_software_adapter_is_never_chosen() {
        let only = [software()];
        for preference in [Preference::Dedicated, Preference::Integrated] {
            assert_eq!(preference.rank(&only[0]), None);
            assert_eq!(pick(preference, &only), None);
        }
        // Not even beside a card it reports more memory than: no encoder runs
        // on the Basic Render Driver.
        let adapters = [software(), card(1, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(1));
    }

    #[test]
    fn an_igpu_reporting_most_of_ram_as_shared_is_still_an_igpu() {
        // The classic misclassification: an Intel UHD 770 offers 128 MiB of
        // dedicated memory and calls half the machine's RAM its own shared
        // memory. Read as "a lot of memory, so a card", a stream would land on
        // the iGPU. The vendor says which kind of GPU it is; memory only sorts
        // adapters already known to be the same kind, so 8 GiB of shared
        // memory changes nothing here.
        let uhd = on_die(2, INTEL);
        assert_eq!(uhd.dedicated_memory, 128 << 20);
        assert_eq!(uhd.shared_memory, 8 * GIB);
        assert_eq!(Preference::Dedicated.rank(&uhd), Some(Rank::Fallback));
        assert_eq!(Preference::Integrated.rank(&uhd), Some(Rank::Preferred));
        let adapters = [uhd, card(1, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
    }

    #[test]
    fn of_two_cards_the_one_with_more_memory_of_its_own_wins() {
        let adapters = [card(1, NVIDIA, 8 * GIB), card(2, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(2));
        // Whichever order DXGI lists them in.
        let swapped = [adapters[1], adapters[0]];
        assert_eq!(pick(Preference::Dedicated, &swapped), Some(2));
    }

    #[test]
    fn no_adapters_at_all_chooses_nothing() {
        assert_eq!(choose(Preference::Dedicated, Vec::<Adapter>::new()), None);
    }
}
