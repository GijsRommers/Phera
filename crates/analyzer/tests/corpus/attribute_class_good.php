<?php
// Rule: class.notFound; attribute_class_good.
#[Attribute]
class Flag {}
#[Flag]
function operation() {}
