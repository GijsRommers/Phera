<?php
// Rule: class.notFound; deliberate single error: class_new_parenthesized.
function f() { return (new Absent); }
