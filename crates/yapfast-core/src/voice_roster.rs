//! Who is in a voice channel, summed up for the card its row shows under
//! the pointer: how many are there, and how many share their screen or
//! have their camera on.

use crate::chat::VoicePerson;

/// How many people a voice channel holds, and what they share.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Roster {
    pub connected: usize,
    /// Sharing their screen with Go Live.
    pub streaming: usize,
    /// With their camera on.
    pub video: usize,
}

impl Roster {
    /// Counts `people`. This account in the call on another client counts
    /// as connected, without what it shares, as its row shows no icons.
    pub fn of(people: &[VoicePerson]) -> Self {
        people.iter().fold(Self::default(), |roster, person| {
            let shows = !person.elsewhere;
            Self {
                connected: roster.connected + 1,
                streaming: roster.streaming + usize::from(shows && person.streaming),
                video: roster.video + usize::from(shows && person.video),
            }
        })
    }

    /// The card's heading: `1 person`, or `3 people · 1 live · 2 cameras`;
    /// empty for nobody.
    pub fn summary(&self) -> String {
        if self.connected == 0 {
            return String::new();
        }
        let count =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut parts = vec![count(self.connected, "person", "people")];
        if self.streaming > 0 {
            parts.push(format!("{} live", self.streaming));
        }
        if self.video > 0 {
            parts.push(count(self.video, "camera", "cameras"));
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, streaming: bool, video: bool) -> VoicePerson {
        VoicePerson {
            id: name.to_owned(),
            name: name.to_owned(),
            streaming,
            video,
            ..Default::default()
        }
    }

    #[test]
    fn counts_who_is_in_a_voice_channel_and_what_they_share() {
        assert_eq!(Roster::of(&[]), Roster::default());
        assert_eq!(Roster::of(&[]).summary(), "");

        let one = [person("Ann", false, false)];
        assert_eq!(Roster::of(&one).summary(), "1 person");

        let people = [
            person("Ann", true, true),
            person("Bob", true, false),
            person("Cat", false, true),
            person("Dan", false, false),
        ];
        let roster = Roster::of(&people);
        assert_eq!(
            roster,
            Roster {
                connected: 4,
                streaming: 2,
                video: 2
            }
        );
        assert_eq!(roster.summary(), "4 people · 2 live · 2 cameras");

        let camera = [person("Ann", false, true), person("Bob", false, false)];
        assert_eq!(Roster::of(&camera).summary(), "2 people · 1 camera");
    }

    #[test]
    fn this_account_on_another_client_shares_nothing_here() {
        let mut phone = person("Me", true, true);
        phone.elsewhere = true;
        let roster = Roster::of(&[phone, person("Bob", true, false)]);
        assert_eq!(roster.connected, 2);
        assert_eq!(roster.streaming, 1);
        assert_eq!(roster.video, 0);
    }
}
