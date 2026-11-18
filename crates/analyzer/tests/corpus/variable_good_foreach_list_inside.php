<?php
// Rule: variable.undefined; variable good foreach list inside.
foreach ([[1]] as [$x]) { echo $x; }
