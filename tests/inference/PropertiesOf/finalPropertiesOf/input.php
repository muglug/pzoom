<?php
class A {
    /** @var bool */
    public $foo = false;
}

/** @return properties-of<A> */
function returnPropertyOfA() {
    return ["foo" => true];
}
                
