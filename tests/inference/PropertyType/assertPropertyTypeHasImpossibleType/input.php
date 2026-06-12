<?php
class A {
    /** @var ?B */
    public $foo;
}
class B {}
$a = new A();
if (is_string($a->foo)) {}
