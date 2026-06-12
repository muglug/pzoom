<?php
class A {
  /** @var ?string */
  public $foo;
}

class B {
  public function __toString() {
    return "bar";
  }
}

$a = new A();
$a->foo = new B;
