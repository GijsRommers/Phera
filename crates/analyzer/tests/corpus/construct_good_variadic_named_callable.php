<?php
// Rule: scope / names / signatures; construct good variadic named callable.
$c = fn(...$args) => $args; $c(foo: 1);
