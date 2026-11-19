--TEST--
Default trim characters and an explicit character list
--FILE--
<?php
var_dump(
    bin2hex(trim("\f a \f")),
    bin2hex(ltrim("\f a \f")),
    bin2hex(rtrim("\f a \f")),
    bin2hex(chop("\f a \f")),
    bin2hex(trim("\f a \f", " "))
);
--EXPECT--
string(10) "0c2061200c"
string(10) "0c2061200c"
string(10) "0c2061200c"
string(10) "0c2061200c"
string(10) "0c2061200c"
