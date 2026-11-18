<?php
// Rule: variable.undefined; deliberate single error: this_static_arrow.
$f = static fn() => $this;
