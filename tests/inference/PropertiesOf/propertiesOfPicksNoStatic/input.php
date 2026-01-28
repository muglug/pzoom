<?php
class A {
    /** @var mixed */
    public static $foo;
}

/** @return properties-of<A> */
function returnPropertyOfA() {
    return ["foo" => true];
}
                
