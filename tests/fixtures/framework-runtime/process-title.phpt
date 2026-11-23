--TEST--
The initial CLI process title is empty
--FILE--
<?php
var_dump(cli_get_process_title());
--EXPECT--
string(0) ""
