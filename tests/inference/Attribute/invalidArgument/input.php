<?php
#[Attribute]
class Foo
{
    public function __construct(int $i)
    {
    }
}

#[Foo("foo")]
class Bar{}
