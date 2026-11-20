--TEST--
Stringable values can be assigned to native string properties
--FILE--
<?php
final class TextValue {
    public function __toString(): string {
        return 'converted';
    }
}

$document = new DOMDocument();
try {
    $document->version = new TextValue();
    echo $document->version, "\n";
} catch (Throwable $exception) {
    echo get_class($exception), ': ', $exception->getMessage(), "\n";
}
--EXPECT--
converted
