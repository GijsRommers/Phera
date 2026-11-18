<?php
// Rule: variable.undefined; deliberate single error: variable_closure_capture.
function f() { return function () use ($unavailable) {}; }
