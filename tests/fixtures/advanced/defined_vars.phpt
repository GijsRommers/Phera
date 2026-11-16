--TEST--
defined_vars
--FILE--
<?php
$a=1;$b=2;$vars=get_defined_vars();echo $vars['a'],$vars['b'];
--EXPECT--
12