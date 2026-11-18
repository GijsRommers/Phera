<?php
// Rule: function.notFound / arguments.count; function good introspection.
function f() { return func_get_args(); } f(1, 2);
