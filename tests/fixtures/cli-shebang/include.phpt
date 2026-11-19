--TEST--
CLI includes use the CLI scanner and skip only the first shebang line
--FILE--
<?php
$file = tempnam(sys_get_temp_dir(), 'php');
file_put_contents($file, "#!/usr/bin/env php\n#!another line\n<?php echo __LINE__, PHP_EOL;");
include $file;
unlink($file);
?>
--EXPECT--
#!another line
3
