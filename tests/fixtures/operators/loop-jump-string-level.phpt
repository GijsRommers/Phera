--TEST--
Quoted loop levels remain invalid integers
--FILE--
<?php
while (true) {
    continue
        "1";
}
--EXPECTF--
Fatal error: 'continue' operator accepts only positive integers in %s on line 4
