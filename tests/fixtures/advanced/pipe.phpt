--TEST--
pipe
--FILE--
<?php
function twice($x){return $x*2;}echo 3 |> twice(...) |> twice(...);
--EXPECT--
12