<?php

function takesAttr(DOMAttr $attr): string
{
    return $attr->name;
}

function f(DOMDocument $dom_document, DOMElement $el): void
{
    echo strlen($el->localName);

    $item = $dom_document->getElementsByTagName('psalm')->item(0);
    assert($item !== null);
    $attributes = $item->attributes;

    foreach ($attributes as $attribute) {
        echo $attribute->name;
        takesAttr($attribute);
    }
}
