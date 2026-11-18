--TEST--
Missing return is null
--FILE--
<?php
var_dump(eval('echo "inside\\n";'));
--EXPECT--
inside
NULL
