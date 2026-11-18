--TEST--
Compiling noncanonical casts reports the four scanner deprecations
--FILE--
<?php
echo (integer) 1, (double) 2, (binary) 3, (boolean) 4, "\n";
--EXPECTF--

Deprecated: Non-canonical cast (integer) is deprecated, use the (int) cast instead in %s on line 2

Deprecated: Non-canonical cast (double) is deprecated, use the (float) cast instead in %s on line 2

Deprecated: Non-canonical cast (binary) is deprecated, use the (string) cast instead in %s on line 2

Deprecated: Non-canonical cast (boolean) is deprecated, use the (bool) cast instead in %s on line 2
1231
