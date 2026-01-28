<?php
class A {
    /** @var null|A */
    public $parent;
}

class B extends A {}

$a = new A();

while ($a->parent instanceof B) {
    $a = $a->parent;
}
