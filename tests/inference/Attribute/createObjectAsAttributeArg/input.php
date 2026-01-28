<?php
#[Attribute]
class B
{
    public function __construct(?array $listOfB = null) {}
}

#[Attribute(Attribute::TARGET_CLASS)]
class A
{
    /**
     * @param B[] $listOfB
     */
    public function __construct(?array $listOfB = null) {}
}

#[A([new B])]
class C {}
