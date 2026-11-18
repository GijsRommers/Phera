--TEST--
Weak int enum lookup warns and coerces a lossy float string
--FILE--
<?php
enum Code: int { case Zero = 0; case One = 1; }
var_dump(Code::tryFrom("1.5") === Code::One);
--EXPECTF--
Deprecated: Implicit conversion from float-string "1.5" to int loses precision in %s on line 3
bool(true)
