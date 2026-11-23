--TEST--
CSV separators must be a single byte
--FILE--
<?php
try { str_getcsv('a,b', 'é', escape: ''); } catch (ValueError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
str_getcsv(): Argument #2 ($separator) must be a single character
