<?php
// Rule: arguments.count / argument.unknown; arguments good callable unknown.
function f(callable $c) { $c(anything: 1); }
