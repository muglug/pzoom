<?php
#[Attribute]
class SomeAttr
{
    /** @param class-string $class */
    public function __construct(string $class) {}
}

#[SomeAttr(parent::class)]
class A {}
