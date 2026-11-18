<?php
// Rule: class.notFound; deliberate single error: class_parameter_variadic.
function f(Absent ...$values) {}
