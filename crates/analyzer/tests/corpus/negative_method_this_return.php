<?php
// Rule: method.notFound; deliberate single error: method_this_return.
class C { function f() { return $this->unavailable(); } }
