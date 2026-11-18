<?php
// Rule: names / global scope / callable; constant_closure_callable.
const C = static function ($x) { return $x; }; (C)(1);
