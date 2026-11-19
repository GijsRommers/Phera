--TEST--
PHP source tools match highlight_file
--FILE--
<?php
$filename = tempnam(sys_get_temp_dir(), 'phera-highlight-');
file_put_contents($filename, "<?php\n// comment\necho 1; ?>\n<p>&</p>");
ob_start();
$returned = highlight_file($filename);
$output = ob_get_clean();
var_dump($returned, $output === highlight_file($filename, return: true), $output);
unlink($filename);
set_error_handler(function ($number, $message) use ($filename) {
    echo str_replace($filename, 'example.php', $message), "\n";
});
var_dump(highlight_file($filename, true));

--EXPECT--
bool(true)
bool(true)
string(321) "<pre><code style="color: #000000"><span style="color: #0000BB">&lt;?php
</span><span style="color: #FF8000">// comment
</span><span style="color: #007700">echo </span><span style="color: #0000BB">1</span><span style="color: #007700">; </span><span style="color: #0000BB">?&gt;
</span>&lt;p&gt;&amp;&lt;/p&gt;</code></pre>"
highlight_file(example.php): Failed to open stream: No such file or directory
highlight_file(): Failed opening 'example.php' for highlighting
bool(false)
