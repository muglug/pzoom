<?php
class Attr2 {
    /** @var string */
    public $name = '';
}

function foo(Attr2 $attribute): void {
    $deprecated_attributes = [];

    if (in_array($attribute->name, $deprecated_attributes, true)) {
        echo "deprecated";
    }
}
