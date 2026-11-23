//! Compare schema validation results and libxml diagnostics with PHP 8.5.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn local_schemas_and_defaults() {
    let Some(oracle) = Comparison::from_environment("dom-schema") else {
        return;
    };
    oracle.check_matrix(SCHEMAS, 48);
}

const SCHEMAS: &str = r#"<?php
chdir(__DIR__);
$schema = <<<'XSD'
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:sample"
           elementFormDefault="qualified">
  <xs:element name="order">
    <xs:complexType>
      <xs:sequence><xs:element name="quantity" type="xs:int"/></xs:sequence>
      <xs:attribute name="id" use="required"/>
      <xs:attribute name="country" default="CA"/>
    </xs:complexType>
  </xs:element>
</xs:schema>
XSD;
$file = 'order schema.xsd';
file_put_contents($file, $schema);
$include = '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:sample">'
    .'<xs:include schemaLocation="order%20schema.xsd"/></xs:schema>';
$invalidInclude = str_replace('%20', ' ', $include);
$url = 'file://'.implode('/', array_map('rawurlencode', explode('/', getcwd()))).'/order%20schema.xsd';
$import = '<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">'
    .'<xs:import namespace="urn:sample" schemaLocation="'.$url.'"/></xs:schema>';
libxml_use_internal_errors(true);

foreach ([
    '<order xmlns="urn:sample" id="1"><quantity>7</quantity></order>',
    '<order xmlns="urn:sample" id="1" country="US"><quantity>-2</quantity></order>',
    '<order xmlns="urn:sample"><quantity>7</quantity></order>',
    '<order xmlns="urn:sample" id="1" extra="yes"><quantity>7</quantity></order>',
    '<order xmlns="urn:sample" id="1"><quantity>x</quantity></order>',
    '<order xmlns="urn:other" id="1"><quantity>7</quantity></order>',
] as $index => $xml) {
    foreach ([$file, $include, $import, $invalidInclude] as $mode => $source) {
        foreach ([0, LIBXML_SCHEMA_CREATE] as $flag) {
            $dom = new DOMDocument;
            $dom->loadXML($xml);
            $valid = $mode === 0 ? $dom->schemaValidate($source, $flag)
                : $dom->schemaValidateSource($source, $flag);
            $errors = [];
            foreach (libxml_get_errors() as $error) {
                $errors[] = [$error->level, $error->code, $error->line, $error->column, $error->message];
            }
            echo "$index.$mode.$flag=", json_encode([$valid, $dom->saveXML($dom->documentElement), $errors]), "\n";
            libxml_clear_errors();
        }
    }
}
"#;
