<?php
class A {
    /** @var ?string */
    public $a;
}

class B extends A implements I {}

interface I {}

function takeI(I $i) : void {
    if ($i instanceof A) {
        echo $i->a;
        $i->a = "hello";
    }
}
