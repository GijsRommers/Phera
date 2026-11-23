//! Nested RESP replies are converted iteratively, without recursion on PHP data depth.
use super::*;
enum Task {
    Visit(Reply),
    List(usize),
    Map(usize),
    Zip(usize),
}

pub fn convert(reply: Reply, options: &Options) -> NativeResult {
    convert_inner(reply, options, false)
}

pub fn zipped(reply: Reply, options: &Options) -> NativeResult {
    convert_inner(reply, options, true)
}

fn convert_inner(reply: Reply, options: &Options, zip: bool) -> NativeResult {
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
                    tasks.push(if zip {
                        Task::Zip(items.len())
                    } else {
                        Task::List(items.len())
                    });
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
            Task::List(count) | Task::Map(count) | Task::Zip(count) => {
                let values = results.split_off(results.len() - count);
                let mut array = PhpArray::new();
                if matches!(task, Task::Zip(_)) {
                    let mut values = values.into_iter();
                    while let Some(k) = values.next() {
                        if k.is_string()
                            && let Some(v) = values.next()
                        {
                            array.insert(ArrayKey::from_value(&k)?, v);
                        } else {
                            array.push(k)?;
                        }
                    }
                } else if matches!(task, Task::Map(_)) {
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
