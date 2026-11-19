//! Global constants used by DOM calls.
//!
//! ```php
//! $document->loadXML($xml, LIBXML_NONET);
//! ```

use phera_extension_api::Registry;

pub(in crate::bindings) fn register_libxml_constants(registry: &mut Registry) {
    for (name, value) in [
        ("LIBXML_VERSION", 20913),
        ("LIBXML_RECOVER", 1),
        ("LIBXML_NOENT", 2),
        ("LIBXML_DTDLOAD", 4),
        ("LIBXML_DTDATTR", 8),
        ("LIBXML_DTDVALID", 16),
        ("LIBXML_NOERROR", 32),
        ("LIBXML_NOWARNING", 64),
        ("LIBXML_NOBLANKS", 256),
        ("LIBXML_XINCLUDE", 1024),
        ("LIBXML_NSCLEAN", 8192),
        ("LIBXML_NOCDATA", 16384),
        ("LIBXML_NONET", 2048),
        ("LIBXML_PEDANTIC", 128),
        ("LIBXML_COMPACT", 65536),
        ("LIBXML_NOXMLDECL", 2),
        ("LIBXML_PARSEHUGE", 524288),
        ("LIBXML_BIGLINES", 4194304),
        ("LIBXML_NOEMPTYTAG", 4),
        ("LIBXML_SCHEMA_CREATE", 1),
        ("LIBXML_HTML_NOIMPLIED", 8192),
        ("LIBXML_HTML_NODEFDTD", 4),
        ("LIBXML_ERR_NONE", 0),
        ("LIBXML_ERR_WARNING", 1),
        ("LIBXML_ERR_ERROR", 2),
        ("LIBXML_ERR_FATAL", 3),
        ("LIBXML_NO_XXE", 8388608),
    ] {
        registry.constant(name, i64::from(value));
    }
    registry.constant("LIBXML_DOTTED_VERSION", "2.9.13");
    registry.constant("LIBXML_LOADED_VERSION", "20913");
}

pub(in crate::bindings) fn register_dom_constants(registry: &mut Registry) {
    for (name, value) in [
        ("XML_ELEMENT_NODE", 1),
        ("XML_ATTRIBUTE_NODE", 2),
        ("XML_TEXT_NODE", 3),
        ("XML_CDATA_SECTION_NODE", 4),
        ("XML_ENTITY_REF_NODE", 5),
        ("XML_ENTITY_NODE", 6),
        ("XML_PI_NODE", 7),
        ("XML_COMMENT_NODE", 8),
        ("XML_DOCUMENT_NODE", 9),
        ("XML_DOCUMENT_TYPE_NODE", 10),
        ("XML_DOCUMENT_FRAG_NODE", 11),
        ("XML_NOTATION_NODE", 12),
        ("XML_HTML_DOCUMENT_NODE", 13),
        ("XML_DTD_NODE", 14),
        ("XML_ELEMENT_DECL_NODE", 15),
        ("XML_ATTRIBUTE_DECL_NODE", 16),
        ("XML_ENTITY_DECL_NODE", 17),
        ("XML_NAMESPACE_DECL_NODE", 18),
        ("XML_LOCAL_NAMESPACE", 18),
        ("XML_ATTRIBUTE_CDATA", 1),
        ("XML_ATTRIBUTE_ID", 2),
        ("XML_ATTRIBUTE_IDREF", 3),
        ("XML_ATTRIBUTE_IDREFS", 4),
        ("XML_ATTRIBUTE_ENTITY", 6),
        ("XML_ATTRIBUTE_NMTOKEN", 7),
        ("XML_ATTRIBUTE_NMTOKENS", 8),
        ("XML_ATTRIBUTE_ENUMERATION", 9),
        ("XML_ATTRIBUTE_NOTATION", 10),
        ("DOM_PHP_ERR", 0),
        ("DOM_INDEX_SIZE_ERR", 1),
        ("DOMSTRING_SIZE_ERR", 2),
        ("DOM_HIERARCHY_REQUEST_ERR", 3),
        ("DOM_WRONG_DOCUMENT_ERR", 4),
        ("DOM_INVALID_CHARACTER_ERR", 5),
        ("DOM_NO_DATA_ALLOWED_ERR", 6),
        ("DOM_NO_MODIFICATION_ALLOWED_ERR", 7),
        ("DOM_NOT_FOUND_ERR", 8),
        ("DOM_NOT_SUPPORTED_ERR", 9),
        ("DOM_INUSE_ATTRIBUTE_ERR", 10),
        ("DOM_INVALID_STATE_ERR", 11),
        ("DOM_SYNTAX_ERR", 12),
        ("DOM_INVALID_MODIFICATION_ERR", 13),
        ("DOM_NAMESPACE_ERR", 14),
        ("DOM_INVALID_ACCESS_ERR", 15),
        ("DOM_VALIDATION_ERR", 16),
    ] {
        registry.constant(name, i64::from(value));
    }
}
