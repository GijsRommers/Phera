<?php
// Rule: scope / names / signatures; construct good closure ref init.
$f = function () use (&$x) { $x = 1; }; echo $x;
