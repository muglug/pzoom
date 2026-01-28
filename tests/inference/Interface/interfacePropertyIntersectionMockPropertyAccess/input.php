<?php
class A {
    /** @var ?string */
    private $a;
}

/** @psalm-override-property-visibility */
interface I {}

function takeI(I $i) : void {
    if ($i instanceof A) {
        echo $i->a;
        $i->a = "hello";
    }
}
