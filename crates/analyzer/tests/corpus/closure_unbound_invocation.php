<?php
// Rule: variable.undefined; a known direct closure call has no bound object.
$f = function () { return $this; };
$f();
