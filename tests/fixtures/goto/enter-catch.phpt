--TEST--
A jump into a catch skips the exception binding and still runs finally
--FILE--
<?php
goto inside;
try {
    throw new RuntimeException("skipped");
} catch (RuntimeException $e) {
    inside: var_dump(isset($e));
} finally {
    echo "finally\n";
}
--EXPECT--
bool(false)
finally
