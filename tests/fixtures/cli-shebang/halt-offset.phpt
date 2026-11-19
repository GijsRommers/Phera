--TEST--
The CLI shebang still counts toward the compiler halt byte offset
--FILE--
#!/usr/bin/env php
<?php
echo substr(file_get_contents(__FILE__), __COMPILER_HALT_OFFSET__);
__halt_compiler();payload
--EXPECT--
payload
