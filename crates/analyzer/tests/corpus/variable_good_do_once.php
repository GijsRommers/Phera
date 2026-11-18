<?php
// Rule: variable.undefined; variable good do once.
do { $x = 1; } while (rand()); echo $x;
