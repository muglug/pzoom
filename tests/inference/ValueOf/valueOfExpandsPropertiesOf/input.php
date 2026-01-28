<?php
class A {
    /** @var bool */
    public $foo = false;
    /** @var string */
    private $bar = "";
    /** @var int */
    protected $adams = 42;
}

/** @return list<value-of<properties-of<A>>> */
function returnPropertyOfA() {
    return [true, "bar", 42];
}
                
