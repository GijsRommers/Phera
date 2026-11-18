<?php
// Rule: scope / names / signatures; construct good dynamic global.
function f($n) { global $$n; echo $x; }
