--TEST--
Mail rejects malformed fields before starting the sendmail command
--FILE--
<?php
set_error_handler(static function ($severity, $message) {
    echo $message, "\n";
});
$cases = [
    fn () => mail("a\0b", 'subject', 'body'),
    fn () => mail('to', "a\0b", 'body'),
    fn () => mail('to', 'subject', "a\0b"),
    fn () => mail('to', 'subject', 'body', "Header: a\0b"),
    fn () => mail('to', 'subject', 'body', [], "a\0b"),
    fn () => mail('to', 'subject', 'body', [0 => 'text']),
    fn () => mail('to', 'subject', 'body', ['to' => 'replacement']),
    fn () => mail('to', 'subject', 'body', ['Subject' => 'replacement']),
    fn () => mail('to', 'subject', 'body', ['FROM' => ['sender']]),
    fn () => mail('to', 'subject', 'body', ['X Name' => 'bad name']),
    fn () => mail('to', 'subject', 'body', ['X-Field' => "a\r\nBcc: value"]),
    fn () => mail('to', 'subject', 'body', ['X-Field' => "a\nBcc: value"]),
    fn () => mail('to', 'subject', 'body', ['X-Field' => "a\rb"]),
    fn () => mail('to', 'subject', 'body', ['X-Field' => "a\0b"]),
    fn () => mail('to', 'subject', 'body', ['X-Field' => ['label' => 'value']]),
    fn () => mail('to', 'subject', 'body', ['X-Field' => [true]]),
    fn () => mail('to', 'subject', 'body', "X-Field: value\r\n\r\nBcc: injected"),
    fn () => mb_send_mail('to', 'subject', 'body', ['X-Field' => "a\r\nBcc: value"]),
];
foreach ($cases as $send) {
    try {
        var_dump($send());
    } catch (Throwable $error) {
        echo get_class($error), ': ', $error->getMessage(), "\n";
    }
}
--EXPECT--
ValueError: mail(): Argument #1 ($to) must not contain any null bytes
ValueError: mail(): Argument #2 ($subject) must not contain any null bytes
ValueError: mail(): Argument #3 ($message) must not contain any null bytes
ValueError: mail(): Argument #4 ($additional_headers) must not contain any null bytes
ValueError: mail(): Argument #5 ($additional_params) must not contain any null bytes
TypeError: Header name cannot be numeric, 0 given
ValueError: The additional headers cannot contain the "To" header
ValueError: The additional headers cannot contain the "Subject" header
TypeError: Header "from" must be of type string, array given
ValueError: Header name "X Name" contains invalid characters
ValueError: Header "X-Field" contains CRLF characters that are used as a line separator and are not allowed in the header
ValueError: Header "X-Field" contains LF character that is not allowed in the header
ValueError: Header "X-Field" contains CR character that is not allowed in the header
ValueError: Header "X-Field" contains NULL character that is not allowed in the header
TypeError: Header "X-Field" must only contain numeric keys, "label" found
TypeError: Header "X-Field" must only contain values of type string, true found
mail(): Multiple or malformed newlines found in additional_header
bool(false)
ValueError: Header "X-Field" contains CRLF characters that are used as a line separator and are not allowed in the header
