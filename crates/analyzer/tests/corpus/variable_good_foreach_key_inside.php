<?php
// Rule: variable.undefined; variable good foreach key inside.
foreach ([1] as $k => $x) { echo $k; echo $x; }
