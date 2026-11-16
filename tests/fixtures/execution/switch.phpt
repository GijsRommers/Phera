--TEST--
switch
--FILE--
<?php
$a=2;switch($a){case 1:echo 1;break;case 2:echo 2;case 3:echo 3;break;default:echo 0;}
--EXPECT--
23