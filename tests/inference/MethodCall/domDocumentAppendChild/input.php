<?php
$doc = new DOMDocument("1.0");
$node = $doc->createElement("foo");
if ($node instanceof DOMElement) {
    $newnode = $doc->appendChild($node);
    $newnode->setAttribute("bar", "baz");
}
