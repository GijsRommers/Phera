--TEST--
Weak string enum lookup warns and coerces null
--FILE--
<?php
enum Code: string { case Zero = "0"; case One = "1"; }
var_dump(Code::tryFrom(null) === Code::Zero);
--EXPECTF--
Deprecated: Code::tryFrom(): Passing null to parameter #1 ($value) of type string|int is deprecated in %s on line 3
bool(true)
