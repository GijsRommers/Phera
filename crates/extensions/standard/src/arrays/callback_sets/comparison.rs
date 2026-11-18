//! How the set functions with callbacks compare two entries: by key, by
//! value, or both, with the callbacks or as strings.

use std::cmp::Ordering;

use phera_extension_api::{Context, NativeError, Value};

use super::Entry;
use crate::arrays::callback::{BoolResult, UserComparison};
use crate::arrays::ordering::SortFlags;

pub(super) struct SetComparison<'a> {
    pub comparison: UserComparison<'a>,
    pub data: Option<&'a Value>,
    pub keys: Option<&'a Value>,
    pub key_only: bool,
}

impl SetComparison<'_> {
    pub async fn order(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
    ) -> Result<Ordering, NativeError> {
        if let Some(callback) = self.keys {
            self.comparison
                .compare(
                    context,
                    callback,
                    Value::from(left.0.clone()),
                    Value::from(right.0.clone()),
                    BoolResult::AskAgain,
                )
                .await
        } else {
            self.data(context, left, right, BoolResult::AskAgain).await
        }
    }

    /// php_array_intersect leaves its global callback set to the data callback
    /// after a successful associative value comparison. Later key scans in the
    /// same outer iteration therefore use that callback on keys too.
    pub async fn continuing_order(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
        data_compares_keys: bool,
    ) -> Result<Ordering, NativeError> {
        if data_compares_keys && let Some(callback) = self.data {
            return self
                .comparison
                .compare(
                    context,
                    callback,
                    Value::from(left.0.clone()),
                    Value::from(right.0.clone()),
                    BoolResult::AskAgain,
                )
                .await;
        }
        self.order(context, left, right).await
    }

    pub async fn data(
        &mut self,
        context: &mut dyn Context,
        left: &Entry,
        right: &Entry,
        bools: BoolResult,
    ) -> Result<Ordering, NativeError> {
        if let Some(callback) = self.data {
            self.comparison
                .compare(context, callback, left.1.clone(), right.1.clone(), bools)
                .await
        } else {
            SortFlags::STRING.compare(context, &left.1, &right.1)
        }
    }
}
