//! Borrow text cells directly from a PostgreSQL DataRow message.

use std::io;

use fallible_iterator::FallibleIterator;
use postgres_protocol::message::backend::{DataRowBody, RowDescriptionBody};

use super::invalid;

pub(in super::super) struct Cells<'a> {
    bytes: &'a [u8],
    remaining: usize,
}

impl<'a> Cells<'a> {
    pub(in super::super) fn new(row: &'a DataRowBody) -> Self {
        Self {
            bytes: row.buffer(),
            remaining: row.ranges().size_hint().0,
        }
    }

    pub(in super::super) fn len(&self) -> usize {
        self.remaining
    }

    pub(in super::super) fn next(&mut self) -> io::Result<Option<&'a [u8]>> {
        if self.remaining == 0 {
            return Err(invalid(
                "more PostgreSQL columns requested than the row contains",
            ));
        }
        self.remaining -= 1;
        let (length, tail) = self
            .bytes
            .split_at_checked(4)
            .ok_or_else(|| invalid("truncated PostgreSQL cell length"))?;
        let length = i32::from_be_bytes(
            length
                .try_into()
                .map_err(|_| invalid("invalid PostgreSQL cell length"))?,
        );
        self.bytes = tail;
        if length == -1 {
            return Ok(None);
        }
        let length =
            usize::try_from(length).map_err(|_| invalid("invalid PostgreSQL cell length"))?;
        let (cell, tail) = self
            .bytes
            .split_at_checked(length)
            .ok_or_else(|| invalid("PostgreSQL cell exceeds the row length"))?;
        self.bytes = tail;
        Ok(Some(cell))
    }

    pub(in super::super) fn finish(self) -> io::Result<()> {
        if self.remaining != 0 || !self.bytes.is_empty() {
            return Err(invalid("PostgreSQL row contains unconsumed cells"));
        }
        Ok(())
    }
}

pub(in super::super) struct Field {
    pub name: String,
    pub oid: u32,
    pub table: u32,
    pub size: i16,
    pub modifier: i32,
}

pub(in super::super) fn fields(body: &RowDescriptionBody) -> io::Result<Vec<Field>> {
    body.fields()
        .map(|field| {
            if field.format() != 0 {
                return Err(invalid("PostgreSQL returned a binary result column"));
            }
            Ok(Field {
                name: field.name().into(),
                oid: field.type_oid(),
                table: field.table_oid(),
                size: field.type_size(),
                modifier: field.type_modifier(),
            })
        })
        .collect()
}
