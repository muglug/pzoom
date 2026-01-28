<?php
/** @param non-empty-string $XML */
function foo(string $XML) : void {
    $dom = new DOMDocument();
    $dom->loadXML($XML);

    $elements = rand(0, 1) ? $dom->getElementsByTagName("bar") : [];
    foreach ($elements as $element) {
        $element->getElementsByTagName("bat");
    }
}
