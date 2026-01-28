<?php
#[Attribute(Attribute::TARGET_PARAMETER)]
class Foo {}

#[Attribute(Attribute::TARGET_PROPERTY)]
class Bar {}

class Baz
{
    public function __construct(#[Foo, Bar] private int $test) {}
}
