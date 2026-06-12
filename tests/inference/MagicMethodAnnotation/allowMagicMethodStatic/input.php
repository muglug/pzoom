<?php
/** @method static getStatic() */
class C {
    public function __call(string $c, array $args) {}
}

class D extends C {}

$c = (new C)->getStatic();
$d = (new D)->getStatic();
