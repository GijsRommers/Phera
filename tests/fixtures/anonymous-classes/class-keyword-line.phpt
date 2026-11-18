--TEST--
anonymous-classes: class keyword line
--FILE--
<?php
$x = new
    class
    {
    };
echo explode(':', strstr(get_class($x), chr(0)))[1];
--EXPECT--
3$0
