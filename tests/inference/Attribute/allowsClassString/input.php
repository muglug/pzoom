<?php

#[Attribute(Attribute::TARGET_CLASS)]
class Foo
{
    /**
     * @param class-string<Baz> $_className
     */
    public function __construct(string $_className)
    {
    }
}

#[Foo(_className: Baz::class)]
class Baz {}
