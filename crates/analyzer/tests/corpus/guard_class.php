<?php
// Rule: class.notFound; class_exists guards optional extension classes.
if (class_exists("OptionalExtension")) { new OptionalExtension; }
