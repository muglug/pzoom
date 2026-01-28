<?php
class B { }

class C extends B { }

class A {
    /** @var B */
    public $foo;

    public function __construct() {
        $this->foo = new B();
    }
}

$a = new A();

$out = null;

if ($a->foo instanceof C) {
    // do something
}
else {
    $out = $a->foo;
}