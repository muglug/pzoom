<?php
class A {
    /** @var string|null */
    protected $foo;
}

class B extends A {
    /** @var string|null */
    private $foo;
}
