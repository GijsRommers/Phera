<?php
// Rule: variable.undefined; deliberate single error: variable_increment.
function f() { ++$unavailable; }
