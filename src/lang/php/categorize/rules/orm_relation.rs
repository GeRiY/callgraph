//! Eloquent-szerű reláció-definíciók `$this`-en.
use crate::lang::php::categorize::{CallInfo, CategoryRule};
const METHODS: &[&str] = &[
    "hasMany",
    "belongsTo",
    "hasOne",
    "belongsToMany",
    "morphTo",
    "morphMany",
    "morphOne",
    "hasManyThrough",
    "hasOneThrough",
    "morphToMany",
    "morphedByMany",
];
pub struct OrmRelationRule;
impl CategoryRule for OrmRelationRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        (call.is_this && METHODS.contains(&call.target)).then_some("orm_relation")
    }
}
