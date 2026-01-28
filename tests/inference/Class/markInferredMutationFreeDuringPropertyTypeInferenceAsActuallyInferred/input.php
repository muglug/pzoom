<?php
class A {}

/**
 * @psalm-consistent-constructor
 */
abstract class AbstractClass
{
    protected $renderer;

    public function __construct(A $r)
    {
        $this->renderer = $r;
    }
}

class ConcreteClass extends AbstractClass
{
    public function __construct(A $r)
    {
        parent::__construct($r);
    }
}
