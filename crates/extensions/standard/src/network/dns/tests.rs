//! Canned answers for `getmxrr('test', $hosts, $weights)`.

use std::{cell::RefCell, rc::Rc};

use hickory_resolver::proto::op::Message;
use phera_extension_api::Value;

use super::mx_answer;

#[test]
fn mx_uses_only_mail_exchangers_in_successful_answers() {
    const HEADER_AND_QUESTION: &[u8] = b"\0\x01\x81\x80\0\x01\0\0\0\0\0\0\x04test\0\0\x0f\0\x01";
    const ADDRESS: &[u8] = b"\xc0\x0c\0\x01\0\x01\0\0\0\x3c\0\x04\xc0\0\x02\x01";
    const MAIL_EXCHANGER: &[u8] = b"\xc0\x0c\0\x0f\0\x01\0\0\0\x3c\0\x09\0\x0a\x04mail\xc0\x0c";

    for (answers, success) in [
        (&[][..], false),
        (&[ADDRESS][..], false),
        (&[ADDRESS, MAIL_EXCHANGER][..], true),
    ] {
        let mut packet = HEADER_AND_QUESTION.to_vec();
        packet[7] = answers.len() as u8;
        for answer in answers {
            packet.extend_from_slice(answer);
        }
        let response = Message::from_vec(&packet).expect("valid canned response");
        let hosts = Rc::new(RefCell::new(Value::from("previous")));
        let weights = Rc::new(RefCell::new(Value::from("previous")));
        let args = [
            Value::from("test"),
            Value::Reference(hosts.clone()),
            Value::Reference(weights.clone()),
        ];
        let result = mx_answer(&args, &response.answers).expect("valid answers");
        assert!(matches!(result, Value::Bool(found) if found == success));
        let hosts = hosts.borrow();
        let weights = weights.borrow();
        let (Value::Array(hosts), Value::Array(weights)) = (&*hosts, &*weights) else {
            panic!("getmxrr replaces both result variables with arrays");
        };
        if success {
            assert!(
                matches!(hosts.values().next(), Some(Value::String(host)) if host.as_bytes() == b"mail.test")
            );
            assert!(matches!(weights.values().next(), Some(Value::Int(10))));
        } else {
            assert!(hosts.is_empty());
            assert!(weights.is_empty());
        }
    }
}
