--TEST--
A missing class is reported before the non-string dynamic constant name
--FILE--
<?php
$member = 0;
try {
    MissingClass::{$member};
} catch (Throwable $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Error: Class "MissingClass" not found
