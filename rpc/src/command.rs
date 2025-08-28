pub enum ServiceCommand {
    Ping,
    Error(u32, String),
}

pub enum AuthCommand {
    Register(String, String),
}

pub enum GameCommandData {
    Uno(String),
}

pub enum UnoCommand {
    Pickup,
    PlayCard(u32),
}
