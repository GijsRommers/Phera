--TEST--
alternative
--FILE--
<?php
if(true): echo "a"; else: echo "b"; endif;
--EXPECT--
a