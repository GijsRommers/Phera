--TEST--
A binary comment leaves file identity and CRLF line numbers intact
--FILE--
<?php
// raw ÿ and €
echo str_ends_with(__FILE__, "/comments.php"), ":", __LINE__, "\n";
--EXPECT--
1:3
