use crate::model::picture::{Channel, ChannelControl};
pub fn channels(rgb: [u8; 3]) -> [ChannelControl; 3] {
    [
        ChannelControl {
            label: "Red",
            channel: Channel::Red,
            value: rgb[0],
        },
        ChannelControl {
            label: "Green",
            channel: Channel::Green,
            value: rgb[1],
        },
        ChannelControl {
            label: "Blue",
            channel: Channel::Blue,
            value: rgb[2],
        },
    ]
}
