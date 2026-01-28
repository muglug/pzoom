<?php
abstract class BaseAttribute {
    public function __construct(public string $name) {}
}

#[Attribute(Attribute::TARGET_CLASS)]
class Table extends BaseAttribute {}

/** @param class-string $s */
function foo(string $s) : void {
    foreach ((new ReflectionClass($s))->getAttributes(BaseAttribute::class, 2) as $attr) {
        $attribute = $attr->newInstance();
        echo $attribute->name;
    }
}
