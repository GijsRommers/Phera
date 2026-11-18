<?php
$context = stream_context_create(['file' => ['sample' => 1], 'php' => ['flag' => false]]);
var_dump(stream_context_get_options($context));
var_dump(stream_context_set_option($context, 'file', 'sample', ['changed']));
var_dump(stream_context_set_option($context, 'php', 'other', null));
var_dump(stream_context_get_options($context));

$stream = fopen('options.txt', 'w+', context: $context);
var_dump(stream_context_get_options($stream));
var_dump(stream_context_set_option($stream, 'file', 'sample', 42));
var_dump(stream_context_get_options($context));
