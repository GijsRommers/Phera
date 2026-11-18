<?php
// Rule: scope / names / signatures; construct good static ref.
function f() { static $a = 1; $b =& $a; echo $b; }
