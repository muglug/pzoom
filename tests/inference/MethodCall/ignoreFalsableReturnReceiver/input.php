<?php
function f(): string {
    $doc = new DOMDocument('1.0', 'UTF-8');
    $filesNode = $doc->createElement('files');
    $filesNode->setAttribute('psalm-version', "x");
    $doc->appendChild($filesNode);
    return (string) $doc->saveXML();
}
