--TEST--
An omitted constant on an unshipped extension class
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
try {
    var_dump(\Pdo\Pgsql::TRANSACTION_IDLE);
} catch (\Phera\NotSupportedException $error) {
    echo get_class($error), ': ', $error->getMessage(), "\n";
}
--EXPECT--
Phera\NotSupportedException: Pdo\Pgsql::TRANSACTION_IDLE is not supported by Phera: deprecated since PHP 8.5, as it has no effect
