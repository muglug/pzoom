<?php
function iterator(SimpleXMLElement $xml): iterable {
    $children = $xml->children();
    assert($children !== null);
    foreach ($children as $img) {
        yield $img["src"] ?? "";
    }
}
