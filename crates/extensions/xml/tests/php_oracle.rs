//! Compare streaming callbacks, struct arrays and XML diagnostics with PHP.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn xml_event_matrix() {
    let Some(oracle) = Comparison::from_environment("xml") else {
        return;
    };
    let script = oracle.artifacts.join("xml_matrix.php");
    std::fs::write(&script, MATRIX).expect("write the XML parser fixture");
    let matched = oracle
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 202);
    eprintln!("{count} XML parser results matched");
}

const MATRIX: &str = r#"<?php
$examples = [
    '<root><item id="one">hello</item><item id="two"/></root>',
    '<r xmlns:p="urn:p"><p:tag p:attr="a">é</p:tag><child /></r>',
    '<feed>before&amp;after<item/><![CDATA[<&]]><?p data?><!--comment-->text</feed>',
    '<!DOCTYPE r [<!ENTITY local "safe">]><r>&local;</r>',
];
foreach ($examples as $xml) {
    foreach ([false, true] as $namespace) {
        foreach ([false, true] as $folded) {
            foreach ([1, 9, strlen($xml)] as $size) {
                $parser = $namespace ? xml_parser_create_ns(null, '|') : xml_parser_create();
                xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, $folded);
                $events = [];
                xml_set_element_handler($parser, function ($p, $name, $attrs) use (&$events) {
                    $events[] = ['start', $name, $attrs];
                }, function ($p, $name) use (&$events) {
                    $events[] = ['end', $name];
                });
                xml_set_character_data_handler($parser, function ($p, $text) use (&$events) {
                    $events[] = ['text', $text];
                });
                xml_set_start_namespace_decl_handler($parser, function ($p, $prefix, $uri) use (&$events) {
                    $events[] = ['ns', $prefix, $uri];
                });
                xml_set_processing_instruction_handler($parser, function ($p, $name, $data) use (&$events) {
                    $events[] = ['pi', $name, $data];
                });
                for ($offset = 0; $offset < strlen($xml); $offset += $size) {
                    $last = $offset + $size >= strlen($xml);
                    $status = xml_parse($parser, substr($xml, $offset, $size), $last);
                    if (!$status) {
                        break;
                    }
                }
                echo json_encode([
                    $status, $events, xml_get_error_code($parser),
                    xml_get_current_line_number($parser), xml_get_current_column_number($parser),
                    xml_get_current_byte_index($parser),
                ]), "\n";
            }
            $parser = $namespace ? xml_parser_create_ns(null, '|') : xml_parser_create();
            xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, $folded);
            $status = xml_parse_into_struct($parser, $xml, $values, $index);
            echo json_encode([$status, $values, $index]), "\n";
        }
    }
}
foreach (range(-2, 103) as $number) {
    echo json_encode([$number, xml_error_string($number)]), "\n";
}
foreach (['', "\xEF\xBB\xBF"] as $bom) {
    foreach (['', '<?xml version="1.0" encoding="utf-8"?>'] as $decl) {
        foreach (['', '<!DOCTYPE r [<!ATTLIST r type (literal|pattern) "literal">]>'] as $dtd) {
            foreach ([true, false] as $namespace) {
                foreach ([true, false] as $skip) {
                    $parser = $namespace ? xml_parser_create_ns('UTF-8') : xml_parser_create('UTF-8');
                    xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);
                    xml_parser_set_option($parser, XML_OPTION_SKIP_WHITE, $skip);
                    $xml = $bom . $decl . $dtd . "<r><x>&lt;d&gt;\n &lt;e&gt;</x><![CDATA[  ]]></r>";
                    $status = xml_parse_into_struct($parser, $xml, $values, $index);
                    echo json_encode([
                        $status, $values, $index, xml_get_error_code($parser),
                        xml_get_current_byte_index($parser),
                    ]), "\n";
                }
            }
        }
    }
}
"#;
