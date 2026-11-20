<?php
$root = simplexml_load_string('<r><item id="boot">one</item></r>');
$view = $root->item;
$view->rewind();
$reader = XMLReader::fromString('<r><item>reader</item></r>');
$reader->read();
$pending = XMLReader::fromString('<pending/>');
$writer = XMLWriter::toMemory();
$writer->startElement('r');
$events = [];
$parser = xml_parser_create();
xml_set_element_handler($parser, function ($parser, $name) use (&$events) {
    $events[] = $name;
}, null);
xml_parse($parser, '<r>');
libxml_set_external_entity_loader(fn () => 'boot resolver');
libxml_use_internal_errors(true);
simplexml_load_string('<unclosed>');

Phera\snapshot();

$resolver = libxml_get_external_entity_loader();
$previousErrors = libxml_use_internal_errors(true);
$bootError = libxml_get_last_error();
if (isset($_GET['mutate'])) {
    $view[0]['id'] = 'changed';
    libxml_set_external_entity_loader(null);
    simplexml_load_string('<unclosed>');
}
$reader->read();
$pending->read();
$writer->writeElement('item', (string) $root->item['id']);
$writer->endElement();
xml_parse($parser, '<item/></r>', true);
echo json_encode([
    (string) $view->current()['id'],
    (string) $root->item['id'],
    $reader->name,
    $pending->name,
    $writer->outputMemory(),
    $events,
    $resolver(),
    $previousErrors,
    $bootError,
    libxml_get_last_error() !== false,
]);
