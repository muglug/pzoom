<?php
final class A {
    /** @var bool */
    public $foo = false;
    /** @var string */
    private $bar = "";
    /** @var int */
    protected $adams = 42;
}

/** @return properties-of<A> */
function returnPropertyOfA() {
    return ["foo" => true];
}
                
