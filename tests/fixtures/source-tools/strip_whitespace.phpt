--TEST--
PHP source tools match strip_whitespace
--FILE--
<?php
$filename = tempnam(sys_get_temp_dir(), 'phera-strip-');
$source = <<<'SOURCE'
<!-- outside PHP -->
<?php
/** doc */
  echo   'a  b'; // comment
  $x = <<<TEXT
a  b
TEXT;
  echo/**/ $x; /* comment */
?>
<!-- outside PHP -->
SOURCE;
file_put_contents($filename, $source);
echo json_encode(php_strip_whitespace($filename), JSON_UNESCAPED_SLASHES), "\n";
unlink($filename);
set_error_handler(function ($number, $message) use ($filename) {
    echo str_replace($filename, 'example.php', $message), "\n";
});
var_dump(php_strip_whitespace($filename));

--EXPECT--
"<!-- outside PHP -->\n<?php\n echo 'a  b'; $x = <<<TEXT\na  b\nTEXT;\necho $x; ?>\n<!-- outside PHP -->"
php_strip_whitespace(example.php): Failed to open stream: No such file or directory
string(0) ""
