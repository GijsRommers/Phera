<?php
// Rule: class.notFound; deliberate single error: class_new_named_argument.
function f() { return new Absent(value: 1); }
