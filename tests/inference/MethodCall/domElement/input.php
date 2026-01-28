<?php
function foo(DOMElement $e) : ?string {
    $a = $e->getElementsByTagName("bar");
    $b = $a->item(0);
    if (!$b) {
        return null;
    }
    return $b->getAttribute("bat");
}
