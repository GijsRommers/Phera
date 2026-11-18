<?php
// Rule: class.notFound; deliberate single error: class_static_property.
function f() { return Absent::$value; }
