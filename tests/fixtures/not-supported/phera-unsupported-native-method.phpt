--TEST--
An omitted method on a registered native class
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    (new DateTime)->__wakeup();
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: DateTime::__wakeup() is not supported by Phera: deprecated since PHP 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()
