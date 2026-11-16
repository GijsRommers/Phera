--TEST--
coalesce_quiet
--FILE--
<?php
echo $missing['x']['y']??'fallback';
--EXPECT--
fallback