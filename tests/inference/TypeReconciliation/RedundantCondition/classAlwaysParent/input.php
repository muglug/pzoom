<?php
class AParent {}

class A extends AParent {
    public static function load() : A {
        return new A();
    }
}

$a = A::load();

if ($a instanceof AParent) {}
