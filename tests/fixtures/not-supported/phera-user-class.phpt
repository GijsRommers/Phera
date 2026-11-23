--TEST--
Autoloaded user members take priority over omitted extension members
--FILE--
<?php
// This fixture asserts behavior specific to Phera’s deprecated-item policy.
spl_autoload_register(function ($name) {
    if ($name === 'ZipArchive') {
        class ZipArchive { public const FL_RECOMPRESS = 'user'; }
    }
});
echo ZipArchive::FL_RECOMPRESS, "\n";
--EXPECT--
user
