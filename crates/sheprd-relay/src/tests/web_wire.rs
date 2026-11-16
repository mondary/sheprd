//! The listener's web access switch on both transports.

use crate::{
    socket::{FromListener, ToPeer},
    wire::{Malformed, Message, Parse},
};

#[test]
fn the_switch_round_trips_on_quic() {
    for on in [true, false] {
        let bytes = Message::Web(on).encode().unwrap();
        assert_eq!(bytes, [0x22, u8::from(on)]);
        assert_eq!(Message::parse(&bytes), Ok(Parse::Done(Message::Web(on), 2)));
    }
    assert_eq!(Message::parse(&[0x22]), Ok(Parse::Need(2)));
    assert_eq!(Message::parse(&[0x22, 2]), Err(Malformed::Switch));
}

#[test]
fn the_switch_and_web_dials_read_on_websockets() {
    let message: FromListener = serde_json::from_str(r#"{"t":"web","enabled":true}"#).unwrap();
    assert_eq!(message.decode(), Some(Message::Web(true)));
    let incoming = ToPeer::IncomingWeb {
        session: "abc".into(),
    };
    assert_eq!(
        serde_json::to_string(&incoming).unwrap(),
        r#"{"t":"incoming_web","session":"abc"}"#
    );
}
