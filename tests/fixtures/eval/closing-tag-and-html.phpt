--TEST--
Closing tag and html
--FILE--
<?php
eval('echo "code"; ?> inline');
--EXPECT--
code inline