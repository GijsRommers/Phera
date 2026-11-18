<?php
// Rule: class.notFound; deliberate single error: class_instanceof_parameter.
function f($value) { return $value instanceof Absent; }
