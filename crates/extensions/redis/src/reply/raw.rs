//! Nested RESP replies are converted iteratively, without recursion on PHP data depth.
use super::*;
enum Task {
    Visit(Reply),
    List(usize),
    Map(usize),
}

pub fn convert(reply: Reply, options: &Options) -> NativeResult {
    let mut tasks = vec![Task::Visit(reply)];
    let mut results = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(reply) => match reply {
                Reply::Nil => results.push(Value::Bool(false)),
                Reply::Int(i) => results.push(Value::Int(i)),
                Reply::Double(f) => results.push(Value::Float(f)),
                Reply::Boolean(b) => results.push(Value::Bool(b)),
                Reply::BulkString(b) | Reply::BigNumber(b) => results.push(Value::from(b)),
                Reply::Okay => results.push(if options.reply_literal {
                    Value::from("OK")
                } else {
                    Value::Bool(true)
                }),
                Reply::SimpleString(s) => results.push(if options.reply_literal {
                    Value::from(s)
                } else {
                    Value::Bool(true)
                }),
                Reply::VerbatimString { text, .. } => results.push(Value::from(text)),
                Reply::ServerError(_) => results.push(Value::Bool(false)),
                Reply::Attribute { data, .. } => tasks.push(Task::Visit(*data)),
                Reply::Array(items) | Reply::Set(items) | Reply::Push { data: items, .. } => {
                    tasks.push(Task::List(items.len()));
                    tasks.extend(items.into_iter().rev().map(Task::Visit));
                }
                Reply::Map(items) => {
                    tasks.push(Task::Map(items.len() * 2));
                    for (k, v) in items.into_iter().rev() {
                        tasks.push(Task::Visit(v));
                        tasks.push(Task::Visit(k));
                    }
                }
                _ => return Err(crate::connection::exception("Unsupported RESP value")),
            },
            Task::List(count) | Task::Map(count) => {
                let values = results.split_off(results.len() - count);
                let mut array = PhpArray::new();
                if matches!(task, Task::Map(_)) {
                    let mut values = values.into_iter();
                    while let (Some(k), Some(v)) = (values.next(), values.next()) {
                        array.insert(ArrayKey::from_value(&k)?, v);
                    }
                } else {
                    for value in values {
                        array.push(value)?;
                    }
                }
                results.push(Value::from(array));
            }
        }
    }
    Ok(results.pop().unwrap_or(Value::Bool(false)))
}
