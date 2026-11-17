<?php
// Rule: scope / names / signatures; construct good union native.
function f(int|string $x): int|string { return $x; }
