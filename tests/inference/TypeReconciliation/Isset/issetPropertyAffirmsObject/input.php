<?php
class A {
    /** @var ?int */
    public $id;
}

function takesA(?A $a): A {
    if (isset($a->id)) {
        return $a;
    }

    return new A();
}