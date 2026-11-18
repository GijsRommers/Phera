<?php
// Rule: scope / names / signatures; construct good intersection.
function f(Countable&Iterator $x) { $x->anything(); }
