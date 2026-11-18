<?php
// Rule: variable.undefined; variable good extract.
extract(["x" => 1]); echo $x;
