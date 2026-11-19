--TEST--
CLI entry scripts skip the shebang and retain their original line numbers
--FILE--
#!/usr/bin/env php
<?php
echo __LINE__, "\n";
try {
    absent_function();
} catch (Error $error) {
    echo $error->getLine(), "\n";
}
?>
--EXPECT--
3
5
