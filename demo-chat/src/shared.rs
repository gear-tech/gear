#[derive(Clone, Debug, codec::Encode, codec::Decode)]
pub enum RoomMessage {
    Join { under_name: String },
    Yell { text: String },
}

#[derive(Clone, Debug, codec::Encode, codec::Decode)]
pub enum MemberMessage {
    Private(String),
    Room(String),
}
