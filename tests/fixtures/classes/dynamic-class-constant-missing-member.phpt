--TEST--
A missing dynamic constant names the linked class and member
--FILE--
<?php
class Present {}
$member = 'Other';
try {
    Present::{$member};
} catch (Throwable $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Error: Undefined constant Present::Other
