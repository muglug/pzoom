<?php
class B { }

class C extends B { }

class A {
    /** @var string|B */
    public $foo = "";
}

$a = new A();

$out = null;

if (is_string($a->foo)) {

}
elseif ($a->foo instanceof C) {
    // do something
}
else {
    $out = $a->foo;
}