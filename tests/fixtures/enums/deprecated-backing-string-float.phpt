--TEST--
Weak string enum lookup warns and coerces a lossy float
--FILE--
<?php
enum Code: string { case Zero = "0"; case One = "1"; }
var_dump(Code::tryFrom(1.5) === Code::One);
--EXPECTF--
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 3
bool(true)
