<?php
class A {
    /** @var bool */
    public $foo = false;
    /** @var string */
    private $bar = "";
    /** @var int */
    protected $adams = 42;
}

/** @return properties-of<A> */
function returnPropertyOfA(int $visibility) {
    return [
        "foo" => true,
        "bar" => "foo",
        "adams" => 1
    ];
}
                
