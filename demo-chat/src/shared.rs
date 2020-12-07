pub enum RoomMessage {
    Join { under_name: String },
    Yell { text: String },
}

pub enum BotMessage {
    Private(String),
    Room(String),
}
